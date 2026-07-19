use crate::dependency::dependency_lock_hash;
use crate::manifest::{Capability, ManifestError, PluginDependency, PluginKind, PluginManifest};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

const MAX_WAT_SOURCE_BYTES: usize = 256 * 1024;

/// Exact JSON boundary emitted by the untrusted cache-first code writer.
/// It intentionally contains neither a signature nor approval evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneratedPluginDraft {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub kind: PluginKind,
    pub capabilities: Vec<Capability>,
    #[serde(default)]
    pub network_allowlist: Vec<String>,
    #[serde(default)]
    pub dependencies: Vec<PluginDependency>,
    pub entry_point: String,
    #[serde(default)]
    pub config_schema: BTreeMap<String, String>,
    pub wat_source: String,
}

#[derive(Debug, Clone)]
pub struct CompiledPlugin {
    pub manifest: PluginManifest,
    pub wasm: Vec<u8>,
}

impl GeneratedPluginDraft {
    pub fn compile(self) -> Result<CompiledPlugin, GenerationCompileError> {
        if self.wat_source.len() > MAX_WAT_SOURCE_BYTES {
            return Err(GenerationCompileError::SourceTooLarge);
        }
        let wasm =
            wat::parse_str(&self.wat_source).map_err(|_| GenerationCompileError::InvalidWat)?;
        let manifest = PluginManifest {
            name: self.name,
            version: self.version,
            description: self.description,
            author: self.author,
            kind: self.kind,
            capabilities: self.capabilities,
            network_allowlist: self.network_allowlist,
            dependency_lock_hash: dependency_lock_hash(&self.dependencies),
            dependencies: self.dependencies,
            wasm_hash: hex::encode(Sha256::digest(&wasm)),
            entry_point: self.entry_point,
            config_schema: self.config_schema,
        };
        manifest.validate()?;
        Ok(CompiledPlugin { manifest, wasm })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GenerationCompileError {
    #[error("generated WAT source exceeds the 256 KiB compilation boundary")]
    SourceTooLarge,
    #[error("generated plugin is not valid WebAssembly text")]
    InvalidWat,
    #[error(transparent)]
    Manifest(#[from] ManifestError),
}
