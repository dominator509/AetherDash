use crate::approval::VerifiedPluginApproval;
use crate::gate::{GateError, PluginGate};
use crate::generation::CompiledPlugin;
use crate::manifest::{Capability, PluginManifest};
use crate::registry::PluginStatus;
use crate::runtime::ExecutionReport;
use crate::signing::EdSignature;
use sqlx::PgPool;
use std::collections::BTreeSet;

#[derive(Debug, Clone)]
pub struct PgPluginRepository {
    pool: PgPool,
}

impl PgPluginRepository {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn install(&self, manifest: &PluginManifest) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO plugin_manifests \
             (name, version, capabilities, status, manifest, wasm_hash, dependency_lock_hash) \
             VALUES ($1, $2, $3, 'installed', $4, $5, $6)",
        )
        .bind(&manifest.name)
        .bind(&manifest.version)
        .bind(sqlx::types::Json(&manifest.capabilities))
        .bind(sqlx::types::Json(manifest))
        .bind(&manifest.wasm_hash)
        .bind(&manifest.dependency_lock_hash)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn install_generated(&self, plugin: &CompiledPlugin) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO plugin_manifests \
             (name, version, capabilities, status, manifest, wasm_hash, dependency_lock_hash, \
              wasm_artifact, generated) \
             VALUES ($1, $2, $3, 'installed', $4, $5, $6, $7, TRUE)",
        )
        .bind(&plugin.manifest.name)
        .bind(&plugin.manifest.version)
        .bind(sqlx::types::Json(&plugin.manifest.capabilities))
        .bind(sqlx::types::Json(&plugin.manifest))
        .bind(&plugin.manifest.wasm_hash)
        .bind(&plugin.manifest.dependency_lock_hash)
        .bind(&plugin.wasm)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn load_generated_and_run(
        &self,
        name: &str,
        version: &str,
        gate: &PluginGate,
    ) -> Result<ExecutionReport, PersistentLoadError> {
        let wasm = sqlx::query_scalar::<_, Vec<u8>>(
            "SELECT wasm_artifact FROM plugin_manifests \
             WHERE name = $1 AND version = $2 AND generated = TRUE",
        )
        .bind(name)
        .bind(version)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(PersistentLoadError::ArtifactMissing)?;
        self.load_and_run(name, version, &wasm, gate).await
    }

    pub async fn approve(
        &self,
        name: &str,
        version: &str,
        approval: &VerifiedPluginApproval,
    ) -> Result<bool, sqlx::Error> {
        let mut transaction = self.pool.begin().await?;
        let manifest = sqlx::query_scalar::<_, sqlx::types::Json<PluginManifest>>(
            "SELECT manifest FROM plugin_manifests \
             WHERE name = $1 AND version = $2 AND status = 'installed' FOR UPDATE",
        )
        .bind(name)
        .bind(version)
        .fetch_optional(&mut *transaction)
        .await?;
        let Some(sqlx::types::Json(manifest)) = manifest else {
            transaction.rollback().await?;
            return Ok(false);
        };
        let requested: BTreeSet<_> = manifest.capabilities.iter().copied().collect();
        if !approval.matches_manifest(&manifest)
            || !requested.is_subset(approval.granted_capabilities())
        {
            transaction.rollback().await?;
            return Ok(false);
        }
        let result = sqlx::query(
            "UPDATE plugin_manifests SET status = 'approved', approved_by = $3, \
             approval_step_up_id = $4, signature = $5, signer = $6, \
             granted_capabilities = $7, approved_ts = now(), updated_ts = now() \
             WHERE name = $1 AND version = $2 AND status = 'installed'",
        )
        .bind(name)
        .bind(version)
        .bind(approval.actor_id())
        .bind(approval.step_up_id())
        .bind(serde_json::to_string(approval.signature()).map_err(json_error)?)
        .bind(&approval.signature().public_key)
        .bind(sqlx::types::Json(approval.granted_capabilities()))
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(result.rows_affected() == 1)
    }

    /// Locks the approved row, revalidates its signed artifact, executes it,
    /// and records the loaded transition atomically with respect to revocation.
    pub async fn load_and_run(
        &self,
        name: &str,
        version: &str,
        wasm: &[u8],
        gate: &PluginGate,
    ) -> Result<ExecutionReport, PersistentLoadError> {
        let mut transaction = self.pool.begin().await?;
        let row = sqlx::query_as::<
            _,
            (sqlx::types::Json<PluginManifest>, String, sqlx::types::Json<BTreeSet<Capability>>),
        >(
            "SELECT manifest, signature, granted_capabilities \
             FROM plugin_manifests \
             WHERE name = $1 AND version = $2 AND status = 'approved' FOR UPDATE",
        )
        .bind(name)
        .bind(version)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or(PersistentLoadError::NotApproved)?;
        let signature: EdSignature = serde_json::from_str(&row.1)?;
        let report = gate.load_and_run(&row.0 .0, &signature, wasm, row.2 .0)?;
        let result = sqlx::query(
            "UPDATE plugin_manifests SET status = 'loaded', updated_ts = now() \
             WHERE name = $1 AND version = $2 AND status = 'approved'",
        )
        .bind(name)
        .bind(version)
        .execute(&mut *transaction)
        .await?;
        if result.rows_affected() != 1 {
            transaction.rollback().await?;
            return Err(PersistentLoadError::StateChanged);
        }
        transaction.commit().await?;
        Ok(report)
    }

    pub async fn revoke(&self, name: &str, version: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            "UPDATE plugin_manifests SET status = 'revoked', revoked_ts = now(), updated_ts = now() \
             WHERE name = $1 AND version = $2 AND status <> 'revoked'",
        )
        .bind(name)
        .bind(version)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn status(
        &self,
        name: &str,
        version: &str,
    ) -> Result<Option<PluginStatus>, sqlx::Error> {
        let value = sqlx::query_scalar::<_, String>(
            "SELECT status FROM plugin_manifests WHERE name = $1 AND version = $2",
        )
        .bind(name)
        .bind(version)
        .fetch_optional(&self.pool)
        .await?;
        Ok(value.and_then(|status| match status.as_str() {
            "installed" => Some(PluginStatus::Installed),
            "approved" => Some(PluginStatus::Approved),
            "loaded" => Some(PluginStatus::Loaded),
            "revoked" => Some(PluginStatus::Revoked),
            _ => None,
        }))
    }
}

fn json_error(error: serde_json::Error) -> sqlx::Error {
    sqlx::Error::Encode(Box::new(error))
}

#[derive(Debug, thiserror::Error)]
pub enum PersistentLoadError {
    #[error("plugin is not approved for loading")]
    NotApproved,
    #[error("generated plugin artifact is missing")]
    ArtifactMissing,
    #[error("plugin state changed during loading")]
    StateChanged,
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    SignatureDecode(#[from] serde_json::Error),
    #[error(transparent)]
    Gate(#[from] GateError),
}
