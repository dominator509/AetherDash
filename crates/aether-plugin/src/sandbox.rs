use crate::manifest::{Capability, PluginManifest};
use serde::{Deserialize, Serialize};

/// Configuration for the WebAssembly sandbox that constrains plugin execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// Maximum heap memory the plugin may allocate (bytes).
    pub max_memory_bytes: u64,
    /// Hard deadline for plugin execution (milliseconds).
    pub max_execution_time_ms: u64,
    /// Capabilities the sandbox will allow at runtime.
    pub allowed_capabilities: Vec<Capability>,
    /// Explicit domain allowlist for outbound HTTP (empty = deny all).
    pub network_allowlist: Vec<String>,
    /// Environment variables injected into the sandbox.
    pub env_vars: Vec<(String, String)>,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            max_memory_bytes: 64 * 1024 * 1024, // 64 MB
            max_execution_time_ms: 5000,        // 5 seconds
            allowed_capabilities: vec![],
            network_allowlist: vec![],
            env_vars: vec![],
        }
    }
}

impl SandboxConfig {
    /// Derive a sandbox configuration from a plugin manifest, granting
    /// only the capabilities the manifest declares.
    pub fn from_manifest(manifest: &PluginManifest) -> Self {
        Self { allowed_capabilities: manifest.capabilities.clone(), ..Default::default() }
    }

    /// Check whether a capability has been granted by this sandbox config.
    pub fn validate_capability(&self, capability: &Capability) -> bool {
        self.allowed_capabilities.contains(capability)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sandbox_restricts_unauthorized_capabilities() {
        let config = SandboxConfig::default();
        assert!(!config.validate_capability(&Capability::NetworkHttp));
    }

    #[test]
    fn sandbox_allows_manifest_capabilities() {
        let manifest = crate::manifest::PluginManifest {
            name: "test".into(),
            version: "1".into(),
            description: "".into(),
            author: "".into(),
            kind: crate::manifest::PluginKind::Scanner,
            capabilities: vec![Capability::ReadMarkets, Capability::SubmitAlerts],
            wasm_hash: "a".repeat(64),
            entry_point: "".into(),
            permissions: vec![],
            config_schema: std::collections::HashMap::new(),
        };
        let config = SandboxConfig::from_manifest(&manifest);
        assert!(config.validate_capability(&Capability::ReadMarkets));
        assert!(config.validate_capability(&Capability::SubmitAlerts));
        assert!(!config.validate_capability(&Capability::ExecutePaper));
    }
}
