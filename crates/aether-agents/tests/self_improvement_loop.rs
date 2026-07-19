#![allow(clippy::expect_used)]

use aether_agents::proposal::{EvidenceKind, ImprovementEvidence, ProposalStore};
use aether_authz::{Action, Actor, ActorKind, EvaluationContext, Grant, Tier, Verdict};
use aether_plugin::{
    sign_manifest, Capability, DependencyScanner, GeneratedPluginDraft, KeyPair, PluginGate,
    PluginKind, PluginRegistry,
};
use rust_decimal::Decimal;
use std::collections::{BTreeMap, HashSet};

fn human_grant() -> Grant {
    Grant {
        id: "human-improvement-grant".into(),
        actor_id: "operator".into(),
        actor_kind: ActorKind::Human,
        tier: Tier::BoundedAutopilot,
        scopes: HashSet::from([
            Action::ApplySelfImprovement.scope().into(),
            Action::ApprovePlugin.scope().into(),
        ]),
        scope_restricted: true,
        expires_at: None,
        revoked_at: None,
    }
}

#[test]
fn metric_to_human_to_generated_plugin_uses_the_full_gate() {
    let human = Actor { id: "operator".into(), kind: ActorKind::Human };
    let grant = human_grant();
    let mut context = EvaluationContext::new(100, Some(&grant));
    context.session_tier = Some(Tier::BoundedAutopilot);
    context.step_up_satisfied = true;

    let mut proposals = ProposalStore::default();
    proposals
        .propose(
            "improvement-1",
            "Generate a bounded market reader",
            "--- /dev/null\n+++ generated-market-reader.wat\n@@ -0,0 +1 @@\n+(module)",
            vec![ImprovementEvidence {
                kind: EvidenceKind::Metric,
                source_id: "aether_scan_cycle_ms:p95:2026-07-18".into(),
                value: Decimal::new(125, 0),
                observed_at: 100,
            }],
        )
        .expect("metric-cited proposal");
    let receipt = proposals
        .authorize_application("improvement-1", &human, context)
        .expect("human application authorization");
    assert_eq!(receipt.actor_id(), "operator");

    let compiled = GeneratedPluginDraft {
        name: "generated-market-reader".into(),
        version: "1.0.0".into(),
        description: "human-authorized generated plugin".into(),
        author: "aether-code-writer".into(),
        kind: PluginKind::Strategy,
        capabilities: vec![Capability::ReadMarkets],
        network_allowlist: vec![],
        dependencies: vec![],
        entry_point: "run".into(),
        config_schema: BTreeMap::new(),
        wat_source: include_str!("../../aether-plugin/examples/read_markets.wat").into(),
    }
    .compile()
    .expect("compile generated draft");
    let key = KeyPair::from_seed(&[77; 32]);
    let signature = sign_manifest(&compiled.manifest, &key).expect("sign after human decision");
    let gate = PluginGate::new([key.public_key_hex()], DependencyScanner::new(vec![]));
    let mut plugins = PluginRegistry::new();
    plugins.install(compiled.manifest).expect("inert install");
    assert!(plugins
        .load_and_run("generated-market-reader", "1.0.0", &compiled.wasm, &gate)
        .is_err());
    plugins
        .approve(
            "generated-market-reader",
            "1.0.0",
            &human,
            context,
            "fresh-plugin-step-up",
            signature,
            [Capability::ReadMarkets],
        )
        .expect("manifest-bound plugin approval");
    let report = plugins
        .load_and_run("generated-market-reader", "1.0.0", &compiled.wasm, &gate)
        .expect("sandboxed generated plugin");
    assert_eq!(report.host_calls, vec![Capability::ReadMarkets]);
}

#[test]
fn proposal_module_has_no_apply_or_process_side_effect_path() {
    let source = include_str!("../src/proposal.rs");
    for forbidden in ["std::fs", "std::process", "Command::new", "write(", "apply_patch"] {
        assert!(!source.contains(forbidden), "forbidden self-modification path: {forbidden}");
    }
    let agent = Actor { id: "agent".into(), kind: ActorKind::Agent };
    let grant = Grant {
        id: "agent-grant".into(),
        actor_id: agent.id.clone(),
        actor_kind: ActorKind::Agent,
        tier: Tier::YoloWithinHardCaps,
        scopes: HashSet::from([Action::ApplySelfImprovement.scope().into()]),
        scope_restricted: true,
        expires_at: None,
        revoked_at: None,
    };
    let mut context = EvaluationContext::new(100, Some(&grant));
    context.step_up_satisfied = true;
    assert_eq!(
        aether_authz::evaluate(&agent, Action::ApplySelfImprovement, context).verdict,
        Verdict::Deny
    );
}
