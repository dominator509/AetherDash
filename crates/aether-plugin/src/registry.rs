use crate::approval::{ApprovalError, VerifiedPluginApproval};
use crate::gate::{GateError, PluginGate};
use crate::manifest::{Capability, PluginKind, PluginManifest};
use crate::runtime::ExecutionReport;
use crate::signing::EdSignature;
use aether_authz::{Actor, EvaluationContext};
use std::collections::{BTreeSet, HashMap};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginStatus {
    Installed,
    Approved,
    Loaded,
    Revoked,
}

#[derive(Debug, Clone)]
pub struct PluginEntry {
    pub manifest: PluginManifest,
    pub signature: Option<EdSignature>,
    pub status: PluginStatus,
    pub granted_capabilities: BTreeSet<Capability>,
    pub approved_by: Option<String>,
    pub approval_step_up_id: Option<String>,
}

#[derive(Debug, Default)]
pub struct PluginRegistry {
    plugins: HashMap<(String, String), PluginEntry>,
}

impl PluginRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn install(&mut self, manifest: PluginManifest) -> Result<(), RegistryError> {
        manifest.validate()?;
        let key = (manifest.name.clone(), manifest.version.clone());
        if self.plugins.contains_key(&key) {
            return Err(RegistryError::AlreadyInstalled);
        }
        self.plugins.insert(
            key,
            PluginEntry {
                manifest,
                signature: None,
                status: PluginStatus::Installed,
                granted_capabilities: BTreeSet::new(),
                approved_by: None,
                approval_step_up_id: None,
            },
        );
        Ok(())
    }

    pub fn approve(
        &mut self,
        name: &str,
        version: &str,
        actor: &Actor,
        context: EvaluationContext<'_>,
        step_up_id: &str,
        signature: EdSignature,
        granted: impl IntoIterator<Item = Capability>,
    ) -> Result<(), RegistryError> {
        let granted: BTreeSet<_> = granted.into_iter().collect();
        let manifest = self
            .get(name, version)
            .ok_or_else(|| RegistryError::NotFound(format!("{name}@{version}")))?
            .manifest
            .clone();
        let approval = VerifiedPluginApproval::verify(
            &manifest,
            actor,
            context,
            step_up_id,
            signature,
            granted.iter().copied(),
        )?;
        let entry = self.entry_mut(name, version)?;
        if entry.status != PluginStatus::Installed {
            return Err(RegistryError::InvalidTransition);
        }
        let requested: BTreeSet<_> = entry.manifest.capabilities.iter().copied().collect();
        if !requested.is_subset(&granted) {
            return Err(RegistryError::CapabilitiesNotApproved);
        }
        entry.status = PluginStatus::Approved;
        entry.signature = Some(approval.clone().into_signature());
        entry.granted_capabilities = granted;
        entry.approved_by = Some(approval.actor_id().to_owned());
        entry.approval_step_up_id = Some(approval.step_up_id().to_owned());
        Ok(())
    }

    pub fn load_and_run(
        &mut self,
        name: &str,
        version: &str,
        wasm: &[u8],
        gate: &PluginGate,
    ) -> Result<ExecutionReport, RegistryError> {
        let entry = self.entry_mut(name, version)?;
        if entry.status != PluginStatus::Approved {
            return Err(RegistryError::InvalidTransition);
        }
        let signature = entry.signature.as_ref().ok_or(RegistryError::MissingSignature)?;
        let report = gate.load_and_run(
            &entry.manifest,
            signature,
            wasm,
            entry.granted_capabilities.iter().copied(),
        )?;
        entry.status = PluginStatus::Loaded;
        Ok(report)
    }

    pub fn revoke(&mut self, name: &str, version: &str) -> Result<(), RegistryError> {
        let entry = self.entry_mut(name, version)?;
        entry.status = PluginStatus::Revoked;
        Ok(())
    }

    #[must_use]
    pub fn get(&self, name: &str, version: &str) -> Option<&PluginEntry> {
        self.plugins.get(&(name.to_owned(), version.to_owned()))
    }

    #[must_use]
    pub fn list_by_kind(&self, kind: &PluginKind) -> Vec<&PluginEntry> {
        self.plugins.values().filter(|entry| entry.manifest.kind == *kind).collect()
    }

    #[must_use]
    pub fn list_loaded(&self) -> Vec<&PluginEntry> {
        self.plugins.values().filter(|entry| entry.status == PluginStatus::Loaded).collect()
    }

    fn entry_mut(&mut self, name: &str, version: &str) -> Result<&mut PluginEntry, RegistryError> {
        self.plugins
            .get_mut(&(name.to_owned(), version.to_owned()))
            .ok_or_else(|| RegistryError::NotFound(format!("{name}@{version}")))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("plugin not found: {0}")]
    NotFound(String),
    #[error("plugin version is already installed")]
    AlreadyInstalled,
    #[error("invalid plugin lifecycle transition")]
    InvalidTransition,
    #[error("operator approval does not cover every requested capability")]
    CapabilitiesNotApproved,
    #[error("approved plugin is missing its human-attached signature")]
    MissingSignature,
    #[error(transparent)]
    Manifest(#[from] crate::manifest::ManifestError),
    #[error(transparent)]
    Gate(#[from] GateError),
    #[error(transparent)]
    Approval(#[from] ApprovalError),
}
