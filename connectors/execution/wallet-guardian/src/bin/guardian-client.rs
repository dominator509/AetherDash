//! Authenticated-client adapter for the Guardian gRPC approval flow.
//!
//! Sensitive values arrive on stdin, never command-line arguments, and are
//! never included in output or errors.

use aether_proto::aether::guardian::v1::wallet_guardian_client::WalletGuardianClient;
use aether_proto::aether::guardian::v1::{
    Approval, ApproveProposalRequest, ProposalRequest, ProposalStatus,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::str::FromStr;
use tonic::metadata::MetadataValue;
use tonic::transport::Endpoint;
use tonic::Request;
use zeroize::Zeroize;

#[derive(Deserialize)]
struct ClientInput {
    endpoint: String,
    proposal_id: String,
    session_token: String,
    reference: String,
    totp: String,
}

impl Drop for ClientInput {
    fn drop(&mut self) {
        self.session_token.zeroize();
        self.reference.zeroize();
        self.totp.zeroize();
    }
}

#[derive(Serialize)]
struct ClientOutput<'a> {
    status: &'static str,
    proposal_id: &'a str,
    guardian_status: i32,
}

#[tokio::main]
async fn main() {
    if run().await.is_err() {
        eprintln!("Guardian approval did not complete");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), ()> {
    let mut raw = String::new();
    std::io::stdin().read_to_string(&mut raw).map_err(|_| ())?;
    let input: ClientInput = serde_json::from_str(&raw).map_err(|_| ())?;
    raw.zeroize();
    if !(input.endpoint.starts_with("http://127.0.0.1:")
        || input.endpoint.starts_with("http://localhost:"))
    {
        return Err(());
    }
    if input.proposal_id.len() != 26 || input.reference.len() < 32 || input.totp.len() != 6 {
        return Err(());
    }
    let endpoint = Endpoint::from_shared(input.endpoint.clone()).map_err(|_| ())?;
    let mut client = WalletGuardianClient::connect(endpoint).await.map_err(|_| ())?;
    let mut authorization_text = format!("Bearer {}", input.session_token);
    let authorization = MetadataValue::from_str(&authorization_text).map_err(|_| ())?;
    authorization_text.zeroize();
    let mut get = Request::new(ProposalRequest { id: input.proposal_id.clone() });
    get.metadata_mut().insert("authorization", authorization.clone());
    let proposal = client.get_proposal(get).await.map_err(|_| ())?.into_inner();
    let mut approve = Request::new(ApproveProposalRequest {
        id: input.proposal_id.clone(),
        approval: Some(Approval {
            totp: input.totp.clone(),
            ts: Utc::now().to_rfc3339(),
            reference: input.reference.clone(),
            expected_proposal_hash: proposal.proposal_hash,
        }),
    });
    approve.metadata_mut().insert("authorization", authorization);
    let approved = client.approve_proposal(approve).await.map_err(|_| ())?.into_inner();
    if approved.status != ProposalStatus::Approved as i32 {
        return Err(());
    }
    let output = serde_json::to_vec(&ClientOutput {
        status: "completed",
        proposal_id: &input.proposal_id,
        guardian_status: approved.status,
    })
    .map_err(|_| ())?;
    std::io::stdout().write_all(&output).map_err(|_| ())?;
    Ok(())
}
