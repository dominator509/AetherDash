use crate::manifest::{PluginDependency, PluginManifest};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Default)]
pub struct DependencyScanner {
    denied: BTreeSet<(String, String)>,
}

impl DependencyScanner {
    #[must_use]
    pub fn new(denied: impl IntoIterator<Item = (String, String)>) -> Self {
        Self { denied: denied.into_iter().collect() }
    }

    pub fn scan(&self, manifest: &PluginManifest) -> Result<(), DependencyError> {
        if dependency_lock_hash(&manifest.dependencies) != manifest.dependency_lock_hash {
            return Err(DependencyError::LockMismatch);
        }
        for dependency in &manifest.dependencies {
            if self.denied.contains(&(dependency.name.clone(), dependency.version.clone())) {
                return Err(DependencyError::KnownVulnerable {
                    name: dependency.name.clone(),
                    version: dependency.version.clone(),
                });
            }
        }
        Ok(())
    }
}

#[must_use]
pub fn dependency_lock_hash(dependencies: &[PluginDependency]) -> String {
    let mut locked: Vec<_> =
        dependencies.iter().map(|item| format!("{}@{}", item.name, item.version)).collect();
    locked.sort();
    hex::encode(Sha256::digest(locked.join("\n").as_bytes()))
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DependencyError {
    #[error("signed dependency lock hash does not match the manifest SBOM")]
    LockMismatch,
    #[error("known-vulnerable dependency refused: {name}@{version}")]
    KnownVulnerable { name: String, version: String },
}
