use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Capabilities that a plugin may request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    ReadMarkets,
    ReadPositions,
    SubmitAlerts,
    AccessBrain,
    ExecutePaper,
    NetworkHttp,
    FileSystem,
}

/// High-level classification of a plugin's purpose.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PluginKind {
    Scanner,
    Strategy,
    Alert,
    Dashboard,
    DataImport,
}

/// Signed plugin manifest describing a WASM module's identity, capabilities,
/// and integrity hash.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub kind: PluginKind,
    pub capabilities: Vec<Capability>,
    /// SHA-256 hex digest of the WASM binary this manifest describes.
    pub wasm_hash: String,
    pub entry_point: String,
    /// Human-readable permission strings (e.g. "read", "write").
    pub permissions: Vec<String>,
    #[serde(default)]
    pub config_schema: HashMap<String, String>,
}

impl PluginManifest {
    /// Validate structural integrity of the manifest.
    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.name.is_empty() {
            return Err(ManifestError::InvalidName);
        }
        if self.version.is_empty() {
            return Err(ManifestError::InvalidVersion);
        }
        if self.capabilities.is_empty() {
            return Err(ManifestError::NoCapabilities);
        }
        if self.wasm_hash.len() != 64 {
            return Err(ManifestError::InvalidHash);
        }
        Ok(())
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ManifestError {
    #[error("plugin name is required")]
    InvalidName,
    #[error("plugin version is required")]
    InvalidVersion,
    #[error("at least one capability is required")]
    NoCapabilities,
    #[error("wasm_hash must be 64 hex characters")]
    InvalidHash,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_manifest() -> PluginManifest {
        PluginManifest {
            name: "test-plugin".into(),
            version: "1.0.0".into(),
            description: "A test plugin".into(),
            author: "test".into(),
            kind: PluginKind::Scanner,
            capabilities: vec![Capability::ReadMarkets],
            wasm_hash: "a".repeat(64),
            entry_point: "main".into(),
            permissions: vec!["read".into()],
            config_schema: HashMap::new(),
        }
    }

    #[test]
    fn valid_manifest_passes() {
        assert!(valid_manifest().validate().is_ok());
    }

    #[test]
    fn empty_name_fails() {
        let mut m = valid_manifest();
        m.name = "".into();
        assert!(m.validate().is_err());
    }

    #[test]
    fn no_capabilities_fails() {
        let mut m = valid_manifest();
        m.capabilities = vec![];
        assert!(m.validate().is_err());
    }

    #[test]
    fn bad_hash_length_fails() {
        let mut m = valid_manifest();
        m.wasm_hash = "short".into();
        assert!(m.validate().is_err());
    }
}
