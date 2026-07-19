#![allow(clippy::expect_used)]

use aether_authz::{Actor, ActorKind, EvaluationContext, Grant, Tier};
use aether_plugin::{
    dependency_lock_hash, sign_manifest, Capability, DependencyScanner, GateError,
    GeneratedPluginDraft, KeyPair, MemoryPluginAudit, PluginDependency, PluginGate, PluginKind,
    PluginManifest, PluginRegistry, PluginStatus, RuntimeError,
};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

fn wasm(wat_source: &str) -> Vec<u8> {
    wat::parse_str(wat_source).expect("valid fixture WAT")
}

fn manifest_for(wasm: &[u8], capabilities: Vec<Capability>) -> PluginManifest {
    let dependencies = Vec::new();
    PluginManifest {
        name: "fixture-plugin".into(),
        version: "1.0.0".into(),
        description: "hostile-suite fixture".into(),
        author: "test".into(),
        kind: PluginKind::Strategy,
        capabilities,
        network_allowlist: Vec::new(),
        dependency_lock_hash: dependency_lock_hash(&dependencies),
        dependencies,
        wasm_hash: hex::encode(Sha256::digest(wasm)),
        entry_point: "run".into(),
        config_schema: BTreeMap::new(),
    }
}

fn approval_grant(actor_id: &str) -> Grant {
    Grant {
        id: "plugin-approval-grant".into(),
        actor_id: actor_id.into(),
        actor_kind: ActorKind::Human,
        tier: Tier::BoundedAutopilot,
        scopes: HashSet::new(),
        scope_restricted: false,
        expires_at: None,
        revoked_at: None,
    }
}

fn context(grant: &Grant, step_up: bool) -> EvaluationContext<'_> {
    let mut context = EvaluationContext::new(100, Some(grant));
    context.session_tier = Some(Tier::BoundedAutopilot);
    context.step_up_satisfied = step_up;
    context.confirmed = true;
    context
}

fn gate(key: &KeyPair, denied: Vec<(String, String)>) -> PluginGate {
    PluginGate::new([key.public_key_hex()], DependencyScanner::new(denied))
}

fn assert_denied_and_logged(
    guarded: PluginGate,
    manifest: &PluginManifest,
    signature: &aether_plugin::PluginSignature,
    bytes: &[u8],
    granted: impl IntoIterator<Item = Capability>,
) {
    let audit = Arc::new(MemoryPluginAudit::default());
    assert!(guarded
        .with_audit(audit.clone())
        .load_and_run(manifest, signature, bytes, granted)
        .is_err());
    let events = audit.events();
    assert_eq!(events.len(), 1);
    assert!(!events[0].allowed);
}

#[test]
fn generated_shape_plugin_loads_only_after_human_step_up_approval() {
    let compiled = GeneratedPluginDraft {
        name: "fixture-plugin".into(),
        version: "1.0.0".into(),
        description: "generated hostile-suite fixture".into(),
        author: "aether-code-writer".into(),
        kind: PluginKind::Strategy,
        capabilities: vec![Capability::ReadMarkets],
        network_allowlist: vec![],
        dependencies: vec![],
        entry_point: "run".into(),
        config_schema: BTreeMap::new(),
        wat_source: include_str!("../examples/read_markets.wat").into(),
    }
    .compile()
    .expect("compile generated draft");
    let bytes = compiled.wasm;
    let manifest = compiled.manifest;
    let key = KeyPair::from_seed(&[7; 32]);
    let signature = sign_manifest(&manifest, &key).expect("sign fixture");
    let mut registry = PluginRegistry::new();
    registry.install(manifest).expect("install pending");
    assert!(registry.get("fixture-plugin", "1.0.0").expect("entry").signature.is_none());
    assert!(registry.load_and_run("fixture-plugin", "1.0.0", &bytes, &gate(&key, vec![])).is_err());

    let actor = Actor { id: "operator".into(), kind: ActorKind::Human };
    let grant = approval_grant(&actor.id);
    registry
        .approve(
            "fixture-plugin",
            "1.0.0",
            &actor,
            context(&grant, true),
            "consumed-step-up-id",
            signature,
            [Capability::ReadMarkets],
        )
        .expect("human approval");
    let report = registry
        .load_and_run("fixture-plugin", "1.0.0", &bytes, &gate(&key, vec![]))
        .expect("load approved plugin");
    assert_eq!(report.return_code, 1);
    assert_eq!(report.host_calls, vec![Capability::ReadMarkets]);
    assert_eq!(
        registry.get("fixture-plugin", "1.0.0").expect("entry").status,
        PluginStatus::Loaded
    );
}

#[test]
fn unsigned_and_manifest_tampering_are_denied_and_audited() {
    let bytes = wasm(r#"(module (func (export "run") (result i32) i32.const 0))"#);
    let mut manifest = manifest_for(&bytes, vec![Capability::ReadMarkets]);
    let key = KeyPair::from_seed(&[15; 32]);
    let audit = Arc::new(MemoryPluginAudit::default());
    let guarded = gate(&key, vec![]).with_audit(audit.clone());
    let unsigned = guarded.load_candidate(&manifest, None, &bytes, [Capability::ReadMarkets]);
    assert!(matches!(unsigned, Err(GateError::MissingSignature)));
    assert_eq!(audit.events().last().expect("unsigned audit").reason, "missing_signature");

    let signature = sign_manifest(&manifest, &key).expect("sign fixture");
    manifest.description = "tampered after signing".into();
    let tampered = guarded.load_and_run(&manifest, &signature, &bytes, [Capability::ReadMarkets]);
    assert!(matches!(tampered, Err(GateError::Signature(_))));
    assert_eq!(audit.events().last().expect("tamper audit").reason, "invalid_signature");
}

#[test]
fn approval_requires_fresh_step_up_and_a_human_actor() {
    let bytes = wasm(r#"(module (func (export "run") (result i32) i32.const 0))"#);
    let manifest = manifest_for(&bytes, vec![Capability::ReadMarkets]);
    let key = KeyPair::from_seed(&[8; 32]);
    let signature = sign_manifest(&manifest, &key).expect("sign fixture");
    let mut registry = PluginRegistry::new();
    registry.install(manifest).expect("install");
    let actor = Actor { id: "operator".into(), kind: ActorKind::Human };
    let grant = approval_grant(&actor.id);
    assert!(registry
        .approve(
            "fixture-plugin",
            "1.0.0",
            &actor,
            context(&grant, false),
            "step-up",
            signature.clone(),
            [Capability::ReadMarkets],
        )
        .is_err());
    let agent = Actor { id: "agent".into(), kind: ActorKind::Agent };
    assert!(registry
        .approve(
            "fixture-plugin",
            "1.0.0",
            &agent,
            context(&grant, true),
            "step-up",
            signature,
            [Capability::ReadMarkets],
        )
        .is_err());
}

#[test]
fn actual_host_call_is_checked_every_time_and_over_scope_is_denied() {
    let bytes = wasm(
        r#"(module
            (import "aether" "submit_alert" (func $submit (result i32)))
            (func (export "run") (result i32) call $submit))"#,
    );
    let manifest = manifest_for(&bytes, vec![Capability::ReadMarkets]);
    let key = KeyPair::from_seed(&[9; 32]);
    let signature = sign_manifest(&manifest, &key).expect("sign fixture");
    let audit = Arc::new(MemoryPluginAudit::default());
    let guarded = gate(&key, vec![]).with_audit(audit.clone());
    let result = guarded.load_and_run(&manifest, &signature, &bytes, [Capability::ReadMarkets]);
    assert!(matches!(
        result,
        Err(GateError::Runtime(RuntimeError::CapabilityDenied(Capability::SubmitAlerts)))
    ));
    assert_eq!(audit.events().last().expect("denial audit").reason, "host_capability_denied");

    let over_scoped = manifest_for(&bytes, vec![Capability::ReadMarkets, Capability::SubmitAlerts]);
    let over_signature = sign_manifest(&over_scoped, &key).expect("sign fixture");
    assert_denied_and_logged(
        gate(&key, vec![]),
        &over_scoped,
        &over_signature,
        &bytes,
        [Capability::ReadMarkets],
    );
}

#[test]
fn allowed_host_call_is_recorded_and_memory_limit_is_enforced() {
    let key = KeyPair::from_seed(&[14; 32]);
    let calling = wasm(
        r#"(module
            (import "aether" "read_markets" (func $read (result i32)))
            (func (export "run") (result i32) call $read))"#,
    );
    let manifest = manifest_for(&calling, vec![Capability::ReadMarkets]);
    let signature = sign_manifest(&manifest, &key).expect("sign fixture");
    let audit = Arc::new(MemoryPluginAudit::default());
    let report = gate(&key, vec![])
        .with_audit(audit.clone())
        .load_and_run(&manifest, &signature, &calling, [Capability::ReadMarkets])
        .expect("allowed host call");
    assert_eq!(report.return_code, 1);
    assert_eq!(report.host_calls, vec![Capability::ReadMarkets]);
    assert_eq!(audit.events().last().expect("allow audit").reason, "allowed");

    let oversized =
        wasm(r#"(module (memory 2000) (func (export "run") (result i32) i32.const 0))"#);
    let oversized_manifest = manifest_for(&oversized, vec![Capability::ReadMarkets]);
    let oversized_signature = sign_manifest(&oversized_manifest, &key).expect("sign fixture");
    let audit = Arc::new(MemoryPluginAudit::default());
    let error = gate(&key, vec![])
        .with_audit(audit.clone())
        .load_and_run(
            &oversized_manifest,
            &oversized_signature,
            &oversized,
            [Capability::ReadMarkets],
        )
        .expect_err("64 MiB memory ceiling");
    assert!(matches!(error, GateError::Runtime(RuntimeError::ImportOrStartDenied)));
    assert!(!audit.events().last().expect("memory denial audit").allowed);
}

#[test]
fn hostile_filesystem_network_tamper_and_fuel_fixtures_are_contained() {
    let key = KeyPair::from_seed(&[10; 32]);
    for source in [
        r#"(module (import "wasi_snapshot_preview1" "fd_write" (func)) (func (export "run") (result i32) i32.const 0))"#,
        r#"(module (import "wasi_snapshot_preview1" "sock_open" (func)) (func (export "run") (result i32) i32.const 0))"#,
    ] {
        let bytes = wasm(source);
        let manifest = manifest_for(&bytes, vec![Capability::ReadMarkets]);
        let signature = sign_manifest(&manifest, &key).expect("sign fixture");
        assert_denied_and_logged(
            gate(&key, vec![]),
            &manifest,
            &signature,
            &bytes,
            [Capability::ReadMarkets],
        );
    }

    let bytes = wasm(r#"(module (func (export "run") (result i32) i32.const 1))"#);
    let manifest = manifest_for(&bytes, vec![Capability::ReadMarkets]);
    let signature = sign_manifest(&manifest, &key).expect("sign fixture");
    let mut tampered = bytes.clone();
    tampered.push(0);
    assert_denied_and_logged(
        gate(&key, vec![]),
        &manifest,
        &signature,
        &tampered,
        [Capability::ReadMarkets],
    );

    let infinite = wasm(r#"(module (func (export "run") (result i32) (loop br 0) i32.const 0))"#);
    let infinite_manifest = manifest_for(&infinite, vec![Capability::ReadMarkets]);
    let infinite_signature = sign_manifest(&infinite_manifest, &key).expect("sign fixture");
    assert_denied_and_logged(
        gate(&key, vec![]),
        &infinite_manifest,
        &infinite_signature,
        &infinite,
        [Capability::ReadMarkets],
    );
}

#[test]
fn vulnerable_or_unlocked_dependencies_and_untrusted_signers_are_refused() {
    let bytes = wasm(r#"(module (func (export "run") (result i32) i32.const 1))"#);
    let key = KeyPair::from_seed(&[11; 32]);
    let mut manifest = manifest_for(&bytes, vec![Capability::ReadMarkets]);
    manifest.dependencies =
        vec![PluginDependency { name: "bad-crate".into(), version: "1.2.3".into() }];
    manifest.dependency_lock_hash = dependency_lock_hash(&manifest.dependencies);
    let signature = sign_manifest(&manifest, &key).expect("sign fixture");
    assert_denied_and_logged(
        gate(&key, vec![("bad-crate".into(), "1.2.3".into())]),
        &manifest,
        &signature,
        &bytes,
        [Capability::ReadMarkets],
    );

    let stranger = KeyPair::from_seed(&[12; 32]);
    let stranger_signature = sign_manifest(&manifest, &stranger).expect("sign fixture");
    assert_denied_and_logged(
        gate(&key, vec![]),
        &manifest,
        &stranger_signature,
        &bytes,
        [Capability::ReadMarkets],
    );

    manifest.dependency_lock_hash = "0".repeat(64);
    let bad_lock_signature = sign_manifest(&manifest, &key).expect("sign fixture");
    assert_denied_and_logged(
        gate(&key, vec![]),
        &manifest,
        &bad_lock_signature,
        &bytes,
        [Capability::ReadMarkets],
    );
}

#[test]
fn revocation_is_immediate_and_prevents_reload() {
    let bytes = wasm(r#"(module (func (export "run") (result i32) i32.const 1))"#);
    let manifest = manifest_for(&bytes, vec![Capability::ReadMarkets]);
    let key = KeyPair::from_seed(&[13; 32]);
    let signature = sign_manifest(&manifest, &key).expect("sign fixture");
    let mut registry = PluginRegistry::new();
    registry.install(manifest).expect("install");
    let actor = Actor { id: "operator".into(), kind: ActorKind::Human };
    let grant = approval_grant(&actor.id);
    registry
        .approve(
            "fixture-plugin",
            "1.0.0",
            &actor,
            context(&grant, true),
            "step-up",
            signature,
            [Capability::ReadMarkets],
        )
        .expect("approve");
    registry.revoke("fixture-plugin", "1.0.0").expect("revoke");
    assert!(registry.load_and_run("fixture-plugin", "1.0.0", &bytes, &gate(&key, vec![])).is_err());
}
