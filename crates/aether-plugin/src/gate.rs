use crate::audit::{PluginAuditEvent, PluginAuditSink, StderrPluginAudit};
use crate::dependency::{DependencyError, DependencyScanner};
use crate::manifest::{Capability, ManifestError, PluginManifest};
use crate::runtime::{ExecutionReport, RuntimeError, WasmRuntime};
use crate::sandbox::SandboxConfig;
use crate::signing::{verify_manifest, EdSignature, SigningError};
use std::collections::BTreeSet;
use std::sync::Arc;

#[derive(Clone)]
pub struct PluginGate {
    trusted_signers: BTreeSet<String>,
    dependency_scanner: DependencyScanner,
    runtime: WasmRuntime,
    audit: Arc<dyn PluginAuditSink>,
}

impl PluginGate {
    #[must_use]
    pub fn new(
        trusted_signers: impl IntoIterator<Item = String>,
        dependency_scanner: DependencyScanner,
    ) -> Self {
        Self {
            trusted_signers: trusted_signers.into_iter().collect(),
            dependency_scanner,
            runtime: WasmRuntime::new(),
            audit: Arc::new(StderrPluginAudit),
        }
    }

    #[must_use]
    pub fn with_audit(mut self, audit: Arc<dyn PluginAuditSink>) -> Self {
        self.audit = audit;
        self
    }

    pub fn load_and_run(
        &self,
        manifest: &PluginManifest,
        signature: &EdSignature,
        wasm: &[u8],
        granted: impl IntoIterator<Item = Capability>,
    ) -> Result<ExecutionReport, GateError> {
        self.load_candidate(manifest, Some(signature), wasm, granted)
    }

    pub fn load_candidate(
        &self,
        manifest: &PluginManifest,
        signature: Option<&EdSignature>,
        wasm: &[u8],
        granted: impl IntoIterator<Item = Capability>,
    ) -> Result<ExecutionReport, GateError> {
        let result = self.load_and_run_inner(manifest, signature, wasm, granted);
        self.audit.record(PluginAuditEvent {
            plugin: audit_identifier(&manifest.name),
            version: audit_identifier(&manifest.version),
            stage: "load",
            allowed: result.is_ok(),
            reason: result.as_ref().map_or_else(|error| error.code(), |_| "allowed"),
        });
        result
    }

    fn load_and_run_inner(
        &self,
        manifest: &PluginManifest,
        signature: Option<&EdSignature>,
        wasm: &[u8],
        granted: impl IntoIterator<Item = Capability>,
    ) -> Result<ExecutionReport, GateError> {
        manifest.validate()?;
        let signature = signature.ok_or(GateError::MissingSignature)?;
        if !self.trusted_signers.contains(&signature.public_key) {
            return Err(GateError::UntrustedSigner);
        }
        verify_manifest(manifest, signature)?;
        self.dependency_scanner.scan(manifest)?;
        let sandbox = SandboxConfig::from_approval(manifest, granted);
        Ok(self.runtime.execute(manifest, wasm, &sandbox)?)
    }
}

fn audit_identifier(value: &str) -> String {
    value
        .chars()
        .filter(|value| value.is_ascii_alphanumeric() || matches!(value, '-' | '_' | '.'))
        .take(128)
        .collect()
}

#[derive(Debug, thiserror::Error)]
pub enum GateError {
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error("plugin manifest has no operator-attached signature")]
    MissingSignature,
    #[error("plugin signer is not trusted by the operator")]
    UntrustedSigner,
    #[error(transparent)]
    Signature(#[from] SigningError),
    #[error(transparent)]
    Dependency(#[from] DependencyError),
    #[error(transparent)]
    Runtime(#[from] RuntimeError),
}

impl GateError {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Manifest(_) => "invalid_manifest",
            Self::MissingSignature => "missing_signature",
            Self::UntrustedSigner => "untrusted_signer",
            Self::Signature(_) => "invalid_signature",
            Self::Dependency(_) => "dependency_denied",
            Self::Runtime(RuntimeError::WasmHashMismatch) => "wasm_hash_mismatch",
            Self::Runtime(RuntimeError::OverScopedManifest) => "capability_scope_denied",
            Self::Runtime(RuntimeError::NetworkHostUnavailable) => "network_host_unavailable",
            Self::Runtime(RuntimeError::InvalidModule) => "invalid_module",
            Self::Runtime(RuntimeError::FuelConfiguration) => "fuel_configuration_failed",
            Self::Runtime(RuntimeError::ImportOrStartDenied) => "import_denied",
            Self::Runtime(RuntimeError::InvalidEntryPoint) => "invalid_entry_point",
            Self::Runtime(RuntimeError::ExecutionTrapped) => "execution_trapped",
            Self::Runtime(RuntimeError::CapabilityDenied(_)) => "host_capability_denied",
            Self::Runtime(RuntimeError::HostConfiguration) => "host_configuration_failed",
        }
    }
}
