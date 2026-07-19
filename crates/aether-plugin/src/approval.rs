use crate::manifest::{Capability, ManifestError, PluginManifest};
use crate::signing::{verify_manifest, EdSignature, SigningError};
use aether_authz::{evaluate, Action, Actor, ActorKind, EvaluationContext, Verdict};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

/// Non-forgeable proof that the canonical authz policy accepted human step-up.
#[derive(Debug, Clone)]
pub struct VerifiedPluginApproval {
    actor_id: String,
    step_up_id: String,
    signature: EdSignature,
    granted_capabilities: BTreeSet<Capability>,
    manifest_digest: [u8; 32],
}

impl VerifiedPluginApproval {
    pub fn verify(
        manifest: &PluginManifest,
        actor: &Actor,
        context: EvaluationContext<'_>,
        step_up_id: &str,
        signature: EdSignature,
        granted_capabilities: impl IntoIterator<Item = Capability>,
    ) -> Result<Self, ApprovalError> {
        manifest.validate()?;
        verify_manifest(manifest, &signature)?;
        if actor.kind != ActorKind::Human || step_up_id.is_empty() {
            return Err(ApprovalError::HumanStepUpRequired);
        }
        let decision = evaluate(actor, Action::ApprovePlugin, context);
        if decision.verdict != Verdict::Allow {
            return Err(ApprovalError::Denied(decision.deciding_rule));
        }
        Ok(Self {
            actor_id: actor.id.clone(),
            step_up_id: step_up_id.to_owned(),
            signature,
            granted_capabilities: granted_capabilities.into_iter().collect(),
            manifest_digest: manifest_digest(manifest)?,
        })
    }

    #[must_use]
    pub fn actor_id(&self) -> &str {
        &self.actor_id
    }

    #[must_use]
    pub fn step_up_id(&self) -> &str {
        &self.step_up_id
    }

    #[must_use]
    pub fn signature(&self) -> &EdSignature {
        &self.signature
    }

    #[must_use]
    pub fn granted_capabilities(&self) -> &BTreeSet<Capability> {
        &self.granted_capabilities
    }

    pub(crate) fn matches_manifest(&self, manifest: &PluginManifest) -> bool {
        manifest_digest(manifest).is_ok_and(|digest| digest == self.manifest_digest)
    }

    #[must_use]
    pub fn into_signature(self) -> EdSignature {
        self.signature
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ApprovalError {
    #[error("plugin approval requires a human and fresh step-up evidence")]
    HumanStepUpRequired,
    #[error("plugin approval denied: {0}")]
    Denied(&'static str),
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error(transparent)]
    Signature(#[from] SigningError),
    #[error("plugin manifest could not be bound to approval evidence")]
    Encoding,
}

fn manifest_digest(manifest: &PluginManifest) -> Result<[u8; 32], ApprovalError> {
    let encoded = serde_json::to_vec(manifest).map_err(|_| ApprovalError::Encoding)?;
    Ok(Sha256::digest(encoded).into())
}
