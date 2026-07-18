use aether_authz::hash_session_token;
use aether_proto::aether::guardian::v1::wallet_guardian_server::WalletGuardian;
use aether_proto::aether::guardian::v1::{
    Approval, ApproveProposalRequest, ProposalStatus, TxSpec as WireTxSpec,
};
use aether_wallet_guardian::grpc::GuardianGrpc;
use aether_wallet_guardian::keystore::KeyStore;
use aether_wallet_guardian::proposal::{proposal_hash, CustodyMode, TxSpec};
use aether_wallet_guardian::totp::{TotpAuthority, TotpError};
use aether_wallet_guardian::worker::BroadcastWorker;
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPoolOptions;
use sqlx::Executor;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tonic::metadata::MetadataValue;
use tonic::Request;

struct TestTotp;

impl TotpAuthority for TestTotp {
    fn verify(&self, secret_ref: &str, code: &str, _now: u64) -> Result<bool, TotpError> {
        Ok(secret_ref == "operator-totp" && code == "123456")
    }
}

#[tokio::test]
#[ignore = "requires migrated Postgres; run through scripts/test-integration.sh"]
async fn broadcast_job_survives_worker_restart_and_confirms_without_resigning() {
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL for integration test");
    let admin = PgPoolOptions::new().max_connections(1).connect(&database_url).await.unwrap();
    let schema = format!("guardian_worker_test_{}", uuid::Uuid::new_v4().simple());
    admin.execute(format!("CREATE SCHEMA {schema}").as_str()).await.unwrap();
    let search_path = format!("SET search_path TO {schema}");
    let pool = PgPoolOptions::new()
        .max_connections(3)
        .after_connect(move |connection, _| {
            let search_path = search_path.clone();
            Box::pin(async move {
                connection.execute(search_path.as_str()).await?;
                Ok(())
            })
        })
        .connect(&database_url)
        .await
        .unwrap();
    sqlx::migrate!("../../../infra/migrations").run(&pool).await.unwrap();

    let user = "01ARZ3NDEKTSV4RRFFQ69G5FC0";
    let grant = "01ARZ3NDEKTSV4RRFFQ69G5FC1";
    let proposal_id = "01ARZ3NDEKTSV4RRFFQ69G5FC2";
    sqlx::query("INSERT INTO users (id,display_name) VALUES ($1,'Worker Operator')")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO permission_grants (id,actor_id,actor_kind,tier,scopes,expires_ts) \
         VALUES ($1,$2,'human',5,'{}',now()+INTERVAL '1 day')",
    )
    .bind(grant)
    .bind(user)
    .execute(&pool)
    .await
    .unwrap();
    let tx = TxSpec {
        chain_id: 31337,
        to: "0x1234567890123456789012345678901234567890".into(),
        value: "0x0".into(),
        data: "0x".into(),
        gas_limit: 21_000,
        max_fee_per_gas: "0x3b9aca00".into(),
        max_priority_fee_per_gas: "0x3b9aca00".into(),
    };
    let hash = proposal_hash(&tx, CustodyMode::GuardianCustody);
    sqlx::query(
        "INSERT INTO guardian_proposals \
         (id,proposer_actor_id,proposer_actor_kind,grant_id,tx_spec,custody_mode,state, \
          policy_trace,proposal_hash,value_delta_usd,approved_at,approval_expires_at,expires_ts) \
         VALUES ($1,$2,'human',$3,$4,'guardian_custody','approved','[]',$5,0,now(), \
                 now()+INTERVAL '60 seconds',now()+INTERVAL '10 minutes')",
    )
    .bind(proposal_id)
    .bind(user)
    .bind(grant)
    .bind(serde_json::to_value(&tx).unwrap())
    .bind(hash)
    .execute(&pool)
    .await
    .unwrap();

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let rpc_address = listener.local_addr().unwrap();
    let confirmed = Arc::new(AtomicBool::new(false));
    let confirmed_server = confirmed.clone();
    let sent_raw = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let sent_raw_server = sent_raw.clone();
    let rpc_thread = std::thread::spawn(move || {
        for _ in 0..5 {
            let (mut stream, _) = listener.accept().unwrap();
            let body = read_http_json_body(&mut stream);
            let request: serde_json::Value = serde_json::from_slice(&body).unwrap();
            let method = request["method"].as_str().unwrap();
            let result = match method {
                "eth_getTransactionCount" => serde_json::json!("0x0"),
                "eth_getTransactionReceipt" if confirmed_server.load(Ordering::SeqCst) => {
                    serde_json::json!({"status":"0x1"})
                }
                "eth_getTransactionReceipt" | "eth_getTransactionByHash" => serde_json::Value::Null,
                "eth_sendRawTransaction" => {
                    let raw = request["params"][0].as_str().unwrap().to_owned();
                    sent_raw_server.lock().unwrap().push(raw.clone());
                    serde_json::json!(
                        aether_wallet_guardian::broadcast::raw_transaction_hash(&raw).unwrap()
                    )
                }
                other => panic!("unexpected RPC method: {other}"),
            };
            let response = serde_json::json!({"jsonrpc":"2.0","id":1,"result":result});
            let response = serde_json::to_vec(&response).unwrap();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n",
                response.len()
            )
            .unwrap();
            stream.write_all(&response).unwrap();
        }
    });
    std::env::set_var("AETHER_GUARDIAN__RPC_31337", format!("http://{rpc_address}"));
    let keystore = Arc::new(KeyStore::new("worker-integration-ephemeral"));
    let first_worker = BroadcastWorker::from_env(pool.clone(), keystore.clone()).unwrap();
    let first = first_worker.run_once().await.unwrap();
    assert_eq!(first.prepared, 1);
    assert_eq!(first.submitted, 1);

    let prepared: (String, i64, i32) = sqlx::query_as(
        "SELECT signed_raw,nonce,attempts FROM guardian_broadcast_jobs WHERE proposal_id=$1",
    )
    .bind(proposal_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(prepared.1, 0);
    assert_eq!(prepared.2, 1);
    assert_eq!(sent_raw.lock().unwrap().as_slice(), std::slice::from_ref(&prepared.0));

    // A fresh worker instance represents a daemon restart. The receipt is now
    // visible; it must confirm the durable job without signing or sending again.
    let stale_proposal = "01ARZ3NDEKTSV4RRFFQ69G5FC3";
    sqlx::query(
        "INSERT INTO guardian_proposals \
         (id,proposer_actor_id,proposer_actor_kind,grant_id,tx_spec,custody_mode,state, \
          policy_trace,proposal_hash,value_delta_usd,expires_ts) \
         VALUES ($1,$2,'human',$3,$4,'guardian_custody','pending','[]',$5,0, \
                 now()-INTERVAL '1 second')",
    )
    .bind(stale_proposal)
    .bind(user)
    .bind(grant)
    .bind(serde_json::to_value(&tx).unwrap())
    .bind(proposal_hash(&tx, CustodyMode::GuardianCustody))
    .execute(&pool)
    .await
    .unwrap();
    confirmed.store(true, Ordering::SeqCst);
    sqlx::query(
        "UPDATE guardian_broadcast_jobs SET next_attempt_ts=now(),updated_ts=now() \
         WHERE proposal_id=$1",
    )
    .bind(proposal_id)
    .execute(&pool)
    .await
    .unwrap();
    let restarted_worker = BroadcastWorker::from_env(pool.clone(), keystore).unwrap();
    let second = restarted_worker.run_once().await.unwrap();
    assert_eq!(second.confirmed, 1);
    assert_eq!(second.expired, 1);
    let proposal_state: String =
        sqlx::query_scalar("SELECT state FROM guardian_proposals WHERE id=$1")
            .bind(proposal_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let job_state: String =
        sqlx::query_scalar("SELECT state FROM guardian_broadcast_jobs WHERE proposal_id=$1")
            .bind(proposal_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(proposal_state, "confirmed");
    assert_eq!(job_state, "confirmed");
    let stale_state: String =
        sqlx::query_scalar("SELECT state FROM guardian_proposals WHERE id=$1")
            .bind(stale_proposal)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stale_state, "expired");
    assert_eq!(sent_raw.lock().unwrap().len(), 1);

    rpc_thread.join().unwrap();
    std::env::remove_var("AETHER_GUARDIAN__RPC_31337");

    // A prepared transaction that was never observed by the chain may expire.
    // Its tail nonce is released while the immutable abandoned job remains as
    // audit evidence, so the next proposal cannot be stranded behind a gap.
    let abandoned_id = "01ARZ3NDEKTSV4RRFFQ69G5FC4";
    let replacement_id = "01ARZ3NDEKTSV4RRFFQ69G5FC5";
    let abandoned_tx = TxSpec { chain_id: 31338, ..tx.clone() };
    let abandoned_raw = aether_wallet_guardian::broadcast::sign_eip1559_transaction(
        &KeyStore::new("abandoned-job-ephemeral"),
        &abandoned_tx,
        0,
        31338,
    )
    .unwrap();
    let abandoned_hash =
        aether_wallet_guardian::broadcast::raw_transaction_hash(&abandoned_raw).unwrap();
    sqlx::query(
        "INSERT INTO guardian_proposals \
         (id,proposer_actor_id,proposer_actor_kind,grant_id,tx_spec,custody_mode,state, \
          policy_trace,proposal_hash,value_delta_usd,approved_at,approval_expires_at,expires_ts) \
         VALUES ($1,$2,'human',$3,$4,'guardian_custody','approved','[]',$5,0,now(), \
                 now()+INTERVAL '200 milliseconds',now()+INTERVAL '10 minutes')",
    )
    .bind(abandoned_id)
    .bind(user)
    .bind(grant)
    .bind(serde_json::to_value(&abandoned_tx).unwrap())
    .bind(proposal_hash(&abandoned_tx, CustodyMode::GuardianCustody))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO guardian_chain_nonces (chain_id,next_nonce) VALUES (31338,1)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO guardian_broadcast_jobs \
         (proposal_id,chain_id,nonce,signed_raw,tx_hash,state) \
         VALUES ($1,31338,0,$2,$3,'prepared')",
    )
    .bind(abandoned_id)
    .bind(&abandoned_raw)
    .bind(&abandoned_hash)
    .execute(&pool)
    .await
    .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;

    let null_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let null_address = null_listener.local_addr().unwrap();
    let null_rpc = std::thread::spawn(move || {
        for _ in 0..2 {
            let (mut stream, _) = null_listener.accept().unwrap();
            let _ = read_http_json_body(&mut stream);
            let response = br#"{"jsonrpc":"2.0","id":1,"result":null}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n",
                response.len()
            )
            .unwrap();
            stream.write_all(response).unwrap();
        }
    });
    std::env::set_var("AETHER_GUARDIAN__RPC_31338", format!("http://{null_address}"));
    let abandon_worker = BroadcastWorker::from_env(
        pool.clone(),
        Arc::new(KeyStore::new("abandon-worker-ephemeral")),
    )
    .unwrap();
    let abandoned = abandon_worker.run_once().await.unwrap();
    assert_eq!(abandoned.abandoned, 1);
    null_rpc.join().unwrap();
    std::env::remove_var("AETHER_GUARDIAN__RPC_31338");
    let released_nonce: i64 =
        sqlx::query_scalar("SELECT next_nonce FROM guardian_chain_nonces WHERE chain_id=31338")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(released_nonce, 0);

    sqlx::query(
        "INSERT INTO guardian_proposals \
         (id,proposer_actor_id,proposer_actor_kind,grant_id,tx_spec,custody_mode,state, \
          policy_trace,proposal_hash,value_delta_usd,approved_at,approval_expires_at,expires_ts) \
         VALUES ($1,$2,'human',$3,$4,'guardian_custody','approved','[]',$5,0,now(), \
                 now()+INTERVAL '60 seconds',now()+INTERVAL '10 minutes')",
    )
    .bind(replacement_id)
    .bind(user)
    .bind(grant)
    .bind(serde_json::to_value(&abandoned_tx).unwrap())
    .bind(proposal_hash(&abandoned_tx, CustodyMode::GuardianCustody))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO guardian_broadcast_jobs \
         (proposal_id,chain_id,nonce,signed_raw,tx_hash,state) \
         VALUES ($1,31338,0,$2,$3,'prepared')",
    )
    .bind(replacement_id)
    .bind(&abandoned_raw)
    .bind(&abandoned_hash)
    .execute(&pool)
    .await
    .unwrap();

    pool.close().await;
    admin.execute(format!("DROP SCHEMA {schema} CASCADE").as_str()).await.unwrap();
    admin.close().await;
}

#[allow(clippy::unwrap_used)]
fn read_http_json_body(stream: &mut std::net::TcpStream) -> Vec<u8> {
    let mut request = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        let read = stream.read(&mut buffer).unwrap();
        assert!(read > 0, "RPC client closed before request body completed");
        request.extend_from_slice(&buffer[..read]);
        let Some(headers_end) = request.windows(4).position(|window| window == b"\r\n\r\n") else {
            continue;
        };
        let headers_end = headers_end + 4;
        let headers = String::from_utf8_lossy(&request[..headers_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                line.to_ascii_lowercase()
                    .strip_prefix("content-length:")
                    .map(str::trim)
                    .and_then(|value| value.parse::<usize>().ok())
            })
            .unwrap();
        if request.len() >= headers_end + content_length {
            return request[headers_end..headers_end + content_length].to_vec();
        }
    }
}

#[tokio::test]
#[ignore = "requires migrated Postgres; run through scripts/test-integration.sh"]
async fn proposal_and_approval_are_durable_bound_and_single_use() {
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL for integration test");
    let admin = PgPoolOptions::new().max_connections(1).connect(&database_url).await.unwrap();
    let schema = format!("guardian_test_{}", uuid::Uuid::new_v4().simple());
    admin.execute(format!("CREATE SCHEMA {schema}").as_str()).await.unwrap();
    let search_path = format!("SET search_path TO {schema}");
    let pool = PgPoolOptions::new()
        .max_connections(3)
        .after_connect(move |connection, _| {
            let search_path = search_path.clone();
            Box::pin(async move {
                connection.execute(search_path.as_str()).await?;
                Ok(())
            })
        })
        .connect(&database_url)
        .await
        .unwrap();
    sqlx::migrate!("../../../infra/migrations").run(&pool).await.unwrap();

    let user = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
    let session = "01ARZ3NDEKTSV4RRFFQ69G5FAW";
    let grant = "01ARZ3NDEKTSV4RRFFQ69G5FAX";
    let approval_id = "01ARZ3NDEKTSV4RRFFQ69G5FAZ";
    let challenge = "01ARZ3NDEKTSV4RRFFQ69G5FB0";
    let token = "human-session-token";
    let reference = "r".repeat(43);
    let reference_hash = hex::encode(Sha256::digest(reference.as_bytes()));

    sqlx::query(
        "INSERT INTO users (id, display_name, totp_secret_ref) VALUES ($1,'Operator','operator-totp')",
    )
    .bind(user)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO sessions \
         (id,user_id,token_hash,expires_ts,tier,origin_kind,idle_expires_ts) \
         VALUES ($1,$2,$3,now()+INTERVAL '1 day',5,'human',now()+INTERVAL '1 day')",
    )
    .bind(session)
    .bind(user)
    .bind(hash_session_token(token))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO permission_grants (id,actor_id,actor_kind,tier,scopes,expires_ts) \
         VALUES ($1,$2,'human',5,'{}',now()+INTERVAL '1 day')",
    )
    .bind(grant)
    .bind(user)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO guardian_reference_prices \
         (id,asset_id,asset_decimals,price_usd,observed_ts,source) \
         VALUES ('01ARZ3NDEKTSV4RRFFQ69G5FAY','eip155:137/native',18,2,now(),'fixture')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let rpc_address = listener.local_addr().unwrap();
    let rpc_thread = std::thread::spawn(move || {
        use std::io::{Read, Write};
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 4096];
        let _ = stream.read(&mut request).unwrap();
        let body = r#"{"jsonrpc":"2.0","id":1,"result":"0x"}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        )
        .unwrap();
    });
    std::env::set_var(
        "AETHER_GUARDIAN__ALLOWED_DESTINATIONS",
        format!(
            "0x1234567890123456789012345678901234567890@{}",
            (chrono::Utc::now() - chrono::Duration::hours(25)).to_rfc3339()
        ),
    );
    std::env::set_var("AETHER_GUARDIAN__RPC_137", format!("http://{rpc_address}"));
    let guardian = GuardianGrpc::from_env(
        pool.clone(),
        KeyStore::new("integration-ephemeral"),
        Arc::new(TestTotp),
    )
    .unwrap();
    let mut forged_precision = Request::new(WireTxSpec {
        to: "0x1234567890123456789012345678901234567890".into(),
        value: "0x2386f26fc10000".into(),
        data: "0x".into(),
        chain_id: "137".into(),
        gas_limit: 21_000,
        max_fee_per_gas: "0x3b9aca00".into(),
        max_priority_fee_per_gas: "0x3b9aca00".into(),
        custody_mode: "guardian_custody".into(),
        asset_id: "eip155:137/native".into(),
        asset_decimals: 28,
    });
    forged_precision
        .metadata_mut()
        .insert("authorization", MetadataValue::try_from(format!("Bearer {token}")).unwrap());
    let rejected = guardian.propose_transaction(forged_precision).await.unwrap_err();
    assert_eq!(rejected.code(), tonic::Code::InvalidArgument);

    let mut propose = Request::new(WireTxSpec {
        to: "0x1234567890123456789012345678901234567890".into(),
        value: "0x2386f26fc10000".into(), // 0.01 native units
        data: "0x".into(),
        chain_id: "137".into(),
        gas_limit: 21_000,
        max_fee_per_gas: "0x3b9aca00".into(),
        max_priority_fee_per_gas: "0x3b9aca00".into(),
        custody_mode: "guardian_custody".into(),
        asset_id: "eip155:137/native".into(),
        asset_decimals: 18,
    });
    propose
        .metadata_mut()
        .insert("authorization", MetadataValue::try_from(format!("Bearer {token}")).unwrap());
    let proposed = guardian.propose_transaction(propose).await.unwrap().into_inner();
    rpc_thread.join().unwrap();
    std::env::remove_var("AETHER_GUARDIAN__ALLOWED_DESTINATIONS");
    std::env::remove_var("AETHER_GUARDIAN__RPC_137");
    assert_eq!(proposed.status, ProposalStatus::Pending as i32);
    let proposal = proposed.id;
    let proposal_hash_value = proposed.proposal_hash;

    sqlx::query(
        "INSERT INTO approval_references \
         (id,token_hash,actor_id,action,target_id,channel,requires_step_up,expires_ts) \
         VALUES ($1,$2,$3,'guardian',$4,'sms',true,now()+INTERVAL '5 minutes')",
    )
    .bind(approval_id)
    .bind(&reference_hash)
    .bind(user)
    .bind(&proposal)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO step_up_challenges \
         (id,token_hash,actor_id,action,target_id,approval_reference_id,expires_ts) \
         VALUES ($1,$2,$3,'guardian_approval',$4,$5,now()+INTERVAL '5 minutes')",
    )
    .bind(challenge)
    .bind(&reference_hash)
    .bind(user)
    .bind(&proposal)
    .bind(approval_id)
    .execute(&pool)
    .await
    .unwrap();

    let request = || {
        let mut request = Request::new(ApproveProposalRequest {
            id: proposal.clone(),
            approval: Some(Approval {
                totp: "123456".into(),
                ts: chrono::Utc::now().to_rfc3339(),
                reference: reference.clone(),
                expected_proposal_hash: proposal_hash_value.clone(),
            }),
        });
        request
            .metadata_mut()
            .insert("authorization", MetadataValue::try_from(format!("Bearer {token}")).unwrap());
        request
    };
    let approved = guardian.approve_proposal(request()).await.unwrap().into_inner();
    assert_eq!(approved.status, 4);
    let replay = guardian.approve_proposal(request()).await.unwrap_err();
    assert_eq!(replay.code(), tonic::Code::FailedPrecondition);

    // Live pending/approved proposals reserve their exposure. Otherwise an
    // actor could stack individually-valid proposals and approve/broadcast all
    // of them after each observed only already-broadcast spend.
    let reserved_tx = TxSpec {
        chain_id: 137,
        to: "0x1234567890123456789012345678901234567890".into(),
        value: "0x0".into(),
        data: "0x".into(),
        gas_limit: 21_000,
        max_fee_per_gas: "0x3b9aca00".into(),
        max_priority_fee_per_gas: "0x3b9aca00".into(),
    };
    sqlx::query(
        "INSERT INTO guardian_proposals \
         (id,proposer_actor_id,proposer_actor_kind,grant_id,tx_spec,custody_mode,state, \
          policy_trace,proposal_hash,value_delta_usd,expires_ts) \
         VALUES ('01ARZ3NDEKTSV4RRFFQ69G5FB1',$1,'human',$2,$3, \
                 'guardian_custody','pending','[]',$4,4997,now()+INTERVAL '10 minutes')",
    )
    .bind(user)
    .bind(grant)
    .bind(serde_json::to_value(&reserved_tx).unwrap())
    .bind(proposal_hash(&reserved_tx, CustodyMode::GuardianCustody))
    .execute(&pool)
    .await
    .unwrap();

    let limit_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let limit_rpc_address = limit_listener.local_addr().unwrap();
    let limit_rpc_thread = std::thread::spawn(move || {
        for _ in 0..2 {
            let (mut stream, _) = limit_listener.accept().unwrap();
            let mut request = [0_u8; 4096];
            let _ = stream.read(&mut request).unwrap();
            let body = r#"{"jsonrpc":"2.0","id":1,"result":"0x"}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        }
    });
    std::env::set_var(
        "AETHER_GUARDIAN__ALLOWED_DESTINATIONS",
        format!(
            "0x1234567890123456789012345678901234567890@{}",
            (chrono::Utc::now() - chrono::Duration::hours(25)).to_rfc3339()
        ),
    );
    std::env::set_var("AETHER_GUARDIAN__RPC_137", format!("http://{limit_rpc_address}"));
    let limit_guardian = GuardianGrpc::from_env(
        pool.clone(),
        KeyStore::new("integration-limit-ephemeral"),
        Arc::new(TestTotp),
    )
    .unwrap();
    let limit_request = || {
        let mut request = Request::new(WireTxSpec {
            to: "0x1234567890123456789012345678901234567890".into(),
            value: "0xde0b6b3a7640000".into(), // 1 native unit = $2 at fixture price
            data: "0x".into(),
            chain_id: "137".into(),
            gas_limit: 21_000,
            max_fee_per_gas: "0x3b9aca00".into(),
            max_priority_fee_per_gas: "0x3b9aca00".into(),
            custody_mode: "guardian_custody".into(),
            asset_id: "eip155:137/native".into(),
            asset_decimals: 18,
        });
        request
            .metadata_mut()
            .insert("authorization", MetadataValue::try_from(format!("Bearer {token}")).unwrap());
        request
    };
    let (first, second) = tokio::join!(
        limit_guardian.propose_transaction(limit_request()),
        limit_guardian.propose_transaction(limit_request()),
    );
    let mut results = [first.unwrap().into_inner(), second.unwrap().into_inner()];
    results.sort_by_key(|proposal| proposal.status);
    limit_rpc_thread.join().unwrap();
    std::env::remove_var("AETHER_GUARDIAN__ALLOWED_DESTINATIONS");
    std::env::remove_var("AETHER_GUARDIAN__RPC_137");
    assert_eq!(results[0].status, ProposalStatus::Pending as i32);
    assert_eq!(results[1].status, ProposalStatus::Denied as i32);
    assert!(results[1].policy_trace.contains("daily limit would be exceeded"));

    let rate_limited_reference = "s".repeat(43);
    let rate_limited_hash = hex::encode(Sha256::digest(rate_limited_reference.as_bytes()));
    sqlx::query(
        "INSERT INTO approval_references \
         (id,token_hash,actor_id,action,target_id,channel,requires_step_up,expires_ts) \
         VALUES ('01ARZ3NDEKTSV4RRFFQ69G5FB2',$1,$2,'guardian',$3, \
                 'sms',true,now()+INTERVAL '5 minutes')",
    )
    .bind(&rate_limited_hash)
    .bind(user)
    .bind(&results[0].id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO step_up_challenges \
         (id,token_hash,actor_id,action,target_id,approval_reference_id,expires_ts) \
         VALUES ('01ARZ3NDEKTSV4RRFFQ69G5FB3',$1,$2,'guardian_approval',$3, \
                 '01ARZ3NDEKTSV4RRFFQ69G5FB2',now()+INTERVAL '5 minutes')",
    )
    .bind(&rate_limited_hash)
    .bind(user)
    .bind(&results[0].id)
    .execute(&pool)
    .await
    .unwrap();
    let rate_limited_request = |totp: &str| {
        let mut request = Request::new(ApproveProposalRequest {
            id: results[0].id.clone(),
            approval: Some(Approval {
                totp: totp.into(),
                ts: chrono::Utc::now().to_rfc3339(),
                reference: rate_limited_reference.clone(),
                expected_proposal_hash: results[0].proposal_hash.clone(),
            }),
        });
        request
            .metadata_mut()
            .insert("authorization", MetadataValue::try_from(format!("Bearer {token}")).unwrap());
        request
    };
    for _ in 0..5 {
        let rejected =
            limit_guardian.approve_proposal(rate_limited_request("000000")).await.unwrap_err();
        assert_eq!(rejected.code(), tonic::Code::FailedPrecondition);
    }
    let guessed_after_limit =
        limit_guardian.approve_proposal(rate_limited_request("123456")).await.unwrap_err();
    assert_eq!(guessed_after_limit.code(), tonic::Code::FailedPrecondition);
    let failed_reference: String = sqlx::query_scalar(
        "SELECT status FROM approval_references WHERE id='01ARZ3NDEKTSV4RRFFQ69G5FB2'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(failed_reference, "failed");

    pool.close().await;
    admin.execute(format!("DROP SCHEMA {schema} CASCADE").as_str()).await.unwrap();
    admin.close().await;
}
