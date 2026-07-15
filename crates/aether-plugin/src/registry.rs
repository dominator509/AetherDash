use crate::manifest::{PluginKind, PluginManifest};
use std::collections::HashMap;

/// In-memory registry of registered plugins and their runtime state.
#[derive(Debug, Default)]
pub struct PluginRegistry {
    plugins: HashMap<String, PluginEntry>,
}

/// A registered plugin with its manifest and current runtime status.
#[derive(Debug, Clone)]
pub struct PluginEntry {
    pub manifest: PluginManifest,
    pub enabled: bool,
    pub signature_valid: bool,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a new plugin. The manifest is validated structurally;
    /// `signature_valid` is determined by the caller after signature verification.
    pub fn register(
        &mut self,
        manifest: PluginManifest,
        signature_valid: bool,
    ) -> Result<(), RegistryError> {
        manifest.validate()?;
        let name = manifest.name.clone();
        self.plugins.insert(
            name.clone(),
            PluginEntry {
                manifest,
                enabled: false,
                signature_valid,
            },
        );
        Ok(())
    }

    /// Enable a previously registered plugin. The plugin must have a valid
    /// signature before it can be enabled.
    pub fn enable(&mut self, name: &str) -> Result<(), RegistryError> {
        let entry = self
            .plugins
            .get_mut(name)
            .ok_or_else(|| RegistryError::NotFound(name.into()))?;
        if !entry.signature_valid {
            return Err(RegistryError::SignatureInvalid);
        }
        entry.enabled = true;
        Ok(())
    }

    /// Disable a running plugin without removing it from the registry.
    pub fn disable(&mut self, name: &str) -> Result<(), RegistryError> {
        let entry = self
            .plugins
            .get_mut(name)
            .ok_or_else(|| RegistryError::NotFound(name.into()))?;
        entry.enabled = false;
        Ok(())
    }

    /// Look up a plugin entry by name.
    pub fn get(&self, name: &str) -> Option<&PluginEntry> {
        self.plugins.get(name)
    }

    /// Return all plugins matching a given kind.
    pub fn list_by_kind(&self, kind: &PluginKind) -> Vec<&PluginEntry> {
        self.plugins
            .values()
            .filter(|e| e.manifest.kind == *kind)
            .collect()
    }

    /// Return all currently enabled plugins.
    pub fn list_enabled(&self) -> Vec<&PluginEntry> {
        self.plugins
            .values()
            .filter(|e| e.enabled)
            .collect()
    }
}

#[derive(thiserror::Error, Debug)]
pub enum RegistryError {
    #[error("plugin not found: {0}")]
    NotFound(String),
    #[error("plugin signature is invalid")]
    SignatureInvalid,
    #[error("manifest error: {0}")]
    Manifest(#[from] crate::manifest::ManifestError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::Capability;

    fn test_manifest(name: &str) -> PluginManifest {
        PluginManifest {
            name: name.into(),
            version: "1.0.0".into(),
            description: "".into(),
            author: "".into(),
            kind: PluginKind::Scanner,
            capabilities: vec![Capability::ReadMarkets],
            wasm_hash: "a".repeat(64),
            entry_point: "".into(),
            permissions: vec![],
            config_schema: HashMap::new(),
        }
    }

    #[test]
    fn register_and_enable_plugin() {
        let mut reg = PluginRegistry::new();
        reg.register(test_manifest("test"), true).unwrap();
        reg.enable("test").unwrap();
        assert!(reg.get("test").unwrap().enabled);
    }

    #[test]
    fn cannot_enable_without_valid_signature() {
        let mut reg = PluginRegistry::new();
        reg.register(test_manifest("bad"), false).unwrap();
        assert!(reg.enable("bad").is_err());
    }

    #[test]
    fn disable_plugin() {
        let mut reg = PluginRegistry::new();
        reg.register(test_manifest("toggle"), true).unwrap();
        reg.enable("toggle").unwrap();
        reg.disable("toggle").unwrap();
        assert!(!reg.get("toggle").unwrap().enabled);
    }

    #[test]
    fn list_enabled_only_returns_enabled() {
        let mut reg = PluginRegistry::new();
        reg.register(test_manifest("a"), true).unwrap();
        reg.register(test_manifest("b"), true).unwrap();
        reg.enable("a").unwrap();
        assert_eq!(reg.list_enabled().len(), 1);
    }
}
