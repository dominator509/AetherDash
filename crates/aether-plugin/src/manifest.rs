use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Closed host capabilities. Filesystem access is structurally unavailable.
#[derive(Debug, Clone, Copy, Hash, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    ReadMarkets,
    ReadPositions,
    SubmitAlerts,
    AccessBrain,
    ExecutePaper,
    NetworkHttp,
}

impl Capability {
    #[must_use]
    pub const fn host_import(self) -> Option<&'static str> {
        match self {
            Self::ReadMarkets => Some("read_markets"),
            Self::ReadPositions => Some("read_positions"),
            Self::SubmitAlerts => Some("submit_alert"),
            Self::AccessBrain => Some("access_brain"),
            Self::ExecutePaper => Some("execute_paper"),
            // Network access needs a separate allowlisted HTTP proxy; there is
            // deliberately no ambient socket import in the Wasm runtime.
            Self::NetworkHttp => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginDependency {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PluginKind {
    Scanner,
    Strategy,
    Alert,
    Dashboard,
    DataImport,
}

/// Signed identity and software bill of materials for one immutable Wasm blob.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub kind: PluginKind,
    pub capabilities: Vec<Capability>,
    pub network_allowlist: Vec<String>,
    pub dependencies: Vec<PluginDependency>,
    pub dependency_lock_hash: String,
    pub wasm_hash: String,
    pub entry_point: String,
    #[serde(default)]
    pub config_schema: BTreeMap<String, String>,
}

impl PluginManifest {
    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.name.is_empty()
            || !self
                .name
                .chars()
                .all(|value| value.is_ascii_alphanumeric() || matches!(value, '-' | '_'))
        {
            return Err(ManifestError::InvalidName);
        }
        if self.version.is_empty() {
            return Err(ManifestError::InvalidVersion);
        }
        if self.capabilities.is_empty() {
            return Err(ManifestError::NoCapabilities);
        }
        let unique: BTreeSet<_> = self.capabilities.iter().copied().collect();
        if unique.len() != self.capabilities.len() {
            return Err(ManifestError::DuplicateCapability);
        }
        validate_sha256(&self.wasm_hash).map_err(|_| ManifestError::InvalidHash)?;
        validate_sha256(&self.dependency_lock_hash)
            .map_err(|_| ManifestError::InvalidDependencyHash)?;
        if self.entry_point.is_empty() {
            return Err(ManifestError::InvalidEntryPoint);
        }
        if self.capabilities.contains(&Capability::NetworkHttp) {
            if self.network_allowlist.is_empty()
                || self.network_allowlist.iter().any(|host| !valid_host(host))
            {
                return Err(ManifestError::InvalidNetworkAllowlist);
            }
        } else if !self.network_allowlist.is_empty() {
            return Err(ManifestError::UnexpectedNetworkAllowlist);
        }
        Ok(())
    }
}

fn validate_sha256(value: &str) -> Result<(), ()> {
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(())
    }
}

fn valid_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && !host.contains(['/', ':', '@', '*'])
        && host.split('.').all(|label| {
            !label.is_empty() && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
}

#[derive(thiserror::Error, Debug, PartialEq, Eq)]
pub enum ManifestError {
    #[error("plugin name must contain only ASCII letters, digits, '-' or '_'")]
    InvalidName,
    #[error("plugin version is required")]
    InvalidVersion,
    #[error("at least one capability is required")]
    NoCapabilities,
    #[error("capabilities must be unique")]
    DuplicateCapability,
    #[error("wasm_hash must be 64 hexadecimal characters")]
    InvalidHash,
    #[error("dependency_lock_hash must be 64 hexadecimal characters")]
    InvalidDependencyHash,
    #[error("entry_point is required")]
    InvalidEntryPoint,
    #[error("network_http requires an exact hostname allowlist")]
    InvalidNetworkAllowlist,
    #[error("network allowlist requires the network_http capability")]
    UnexpectedNetworkAllowlist,
}
