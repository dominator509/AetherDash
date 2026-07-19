#![allow(clippy::expect_used)]

use aether_authz::{Actor, ActorKind, EvaluationContext, Grant, Tier};
use aether_plugin::{
    dependency_lock_hash, sign_manifest, Capability, CompiledPlugin, DependencyScanner, KeyPair,
    PgPluginRepository, PluginGate, PluginKind, PluginManifest, PluginStatus,
    VerifiedPluginApproval,
};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};

#[tokio::test]
#[ignore = "requires migrated Postgres; run through scripts/test-integration.sh"]
async fn install_approval_load_and_revocation_are_durable() {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://aether:aether@localhost:5432/aether".into());
    let pool = sqlx::PgPool::connect(&database_url).await.expect("connect Postgres");
    let repo = PgPluginRepository::new(pool.clone());
    sqlx::query("DELETE FROM plugin_manifests WHERE name = 'ep403-postgres-fixture'")
        .execute(&pool)
        .await
        .expect("clear fixture");

    let wasm = wat::parse_str(r#"(module (func (export "run") (result i32) i32.const 1))"#)
        .expect("fixture WAT");
    let manifest = PluginManifest {
        name: "ep403-postgres-fixture".into(),
        version: "1.0.0".into(),
        description: "durability fixture".into(),
        author: "test".into(),
        kind: PluginKind::Strategy,
        capabilities: vec![Capability::ReadMarkets],
        network_allowlist: vec![],
        dependencies: vec![],
        dependency_lock_hash: dependency_lock_hash(&[]),
        wasm_hash: hex::encode(Sha256::digest(&wasm)),
        entry_point: "run".into(),
        config_schema: BTreeMap::new(),
    };
    let compiled = CompiledPlugin { manifest: manifest.clone(), wasm: wasm.clone() };
    repo.install_generated(&compiled).await.expect("install generated pending");
    assert_eq!(
        repo.status(&manifest.name, &manifest.version).await.expect("status"),
        Some(PluginStatus::Installed)
    );

    let actor = Actor { id: "operator".into(), kind: ActorKind::Human };
    let grant = Grant {
        id: "grant".into(),
        actor_id: actor.id.clone(),
        actor_kind: ActorKind::Human,
        tier: Tier::BoundedAutopilot,
        scopes: HashSet::new(),
        scope_restricted: false,
        expires_at: None,
        revoked_at: None,
    };
    let mut context = EvaluationContext::new(100, Some(&grant));
    context.session_tier = Some(Tier::BoundedAutopilot);
    context.step_up_satisfied = true;
    let key = KeyPair::from_seed(&[42; 32]);
    let approval = VerifiedPluginApproval::verify(
        &manifest,
        &actor,
        context,
        "consumed-step-up-row-id",
        sign_manifest(&manifest, &key).expect("sign"),
        [Capability::ReadMarkets],
    )
    .expect("verify approval");
    assert!(repo.approve(&manifest.name, &manifest.version, &approval).await.expect("approve"));
    let gate = PluginGate::new([key.public_key_hex()], DependencyScanner::new(vec![]));
    let report = repo
        .load_generated_and_run(&manifest.name, &manifest.version, &gate)
        .await
        .expect("durable gate and load");
    assert_eq!(report.return_code, 1);
    assert_eq!(
        repo.status(&manifest.name, &manifest.version).await.expect("status"),
        Some(PluginStatus::Loaded)
    );
    assert!(repo.revoke(&manifest.name, &manifest.version).await.expect("revoke"));
    assert_eq!(
        repo.status(&manifest.name, &manifest.version).await.expect("status"),
        Some(PluginStatus::Revoked)
    );

    sqlx::query("DELETE FROM plugin_manifests WHERE name = $1")
        .bind(&manifest.name)
        .execute(&pool)
        .await
        .expect("clean fixture");
}
