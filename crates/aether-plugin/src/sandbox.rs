use crate::manifest::{Capability, PluginManifest};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Resource and capability limits applied to every fresh Wasm store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    pub max_memory_bytes: usize,
    pub max_fuel: u64,
    pub allowed_capabilities: BTreeSet<Capability>,
    pub network_allowlist: BTreeSet<String>,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            max_memory_bytes: 64 * 1024 * 1024,
            max_fuel: 5_000_000,
            allowed_capabilities: BTreeSet::new(),
            network_allowlist: BTreeSet::new(),
        }
    }
}

impl SandboxConfig {
    #[must_use]
    pub fn from_approval(
        manifest: &PluginManifest,
        granted: impl IntoIterator<Item = Capability>,
    ) -> Self {
        Self {
            allowed_capabilities: granted.into_iter().collect(),
            network_allowlist: manifest.network_allowlist.iter().cloned().collect(),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn permits(&self, capability: Capability) -> bool {
        self.allowed_capabilities.contains(&capability)
    }
}
