Layer: 5 - Execution

# EP-403: Plugin Runtime - Signed Manifests, Sandbox, Capability Host

**Band:** 4xx Cross-cutting | **Phase:** 4 | **Status:** done | **Blocked by:** EP-401

## Purpose / Big Picture
Let the operator (and later the code-writing agent) extend AETHER safely: a plugin runtime where every plugin ships a signed capability manifest, runs sandboxed, is dependency-scanned at load, and has each capability checked at the host boundary on every call. INV-6 made real - extensibility without opening the system.

## Scope
Plugin host (sandbox + capability enforcement), manifest schema + signing/verification, capability model + host boundary checks, dependency scan at load, plugin lifecycle (install/approve/load/revoke) with `plugin_manifests`, a hostile-plugin test suite, an example plugin.

## Non-goals
No specific plugins beyond the example (operator/agent authors them), no code-writing agent (EP-406 uses this runtime), no unsandboxed "trusted" fast path (there isn't one - INV-6).

## Context and Orientation
INV-6: signed, sandboxed, dependency-scanned, capability-scoped from day one. SECURITY.md plugin boundary: capability check on every host call, fs/network denied by default, unsigned or over-scoped plugins fail CI and fail load. SPEC-005: plugin approval/signing is a step-up action (tier 4); the plugin runtime enforces capabilities server-side. Depends on EP-401 for the approval/step-up + capability grant semantics.

## Files to Read First
1. ARCHITECTURE.md INV-6 + section 7 (plugin boundary); SECURITY.md (plugin threat T6, sandbox rules); SPEC-005 (plugin approval step-up, capability scoping).
2. SPEC-002 (`plugin_manifests`); checklists/security-review.md.

## Files to Change (Expected Changed Files)
`server/plugins/**` (host, sandbox, capability enforcement, loader, signing/verify, dep-scan) OR a `crates/aether-plugin-host` if Rust/Wasm-based (Decision-Log the runtime: Wasm/Wasmtime with capability host is the natural fit given the operator's background, but confirm and record), manifest schema, `plugin_manifests` usage, example plugin + hostile-fixture suite, migrations if needed, tests, CHANGELOG, this file.

## Interfaces and Contracts
Manifest: `{name, version, capabilities: [scoped], signature, signer, entry}`; capabilities are a closed, scoped set (e.g., `read:markets`, `read:brain:<filter>`, `net:<allowlist>` - explicitly granted, never ambient). Signing: manifests signed; verification at load; unsigned/over-scoped -> load refused + CI fail. Host boundary: every capability-guarded call checked against the plugin's granted capabilities on EVERY invocation (not just load). Sandbox: no fs/network by default; only declared+granted capabilities.

## Milestones
1. **Runtime + sandbox.** Choose + stand up the sandbox (Wasm/Wasmtime capability host recommended; Decision-Log); default-deny fs/network. Done when: a trivial plugin runs sandboxed; a sandbox-escape-attempt fixture (fs/network/syscall) is denied + logged.
2. **Manifest + signing.** Schema, signing, load-time verification; unsigned/tampered manifest refused. Done when: sign/verify tests; tampered-manifest-refused test; CI check that unsigned plugins fail.
3. **Capability model + host boundary.** Scoped capability set; host checks on every guarded call; over-scoped manifest refused. Done when: per-call enforcement test (a plugin calling beyond its grant is denied at call time); over-scope-refused-at-load test.
4. **Dependency scan at load.** Scan plugin dependencies at load; known-vulnerable -> refuse. Done when: dep-scan integration; vulnerable-fixture-refused test.
5. **Lifecycle + approval.** install -> approve (tier-4 step-up, EP-401) -> load -> revoke; `plugin_manifests` status transitions; revoked plugin can't load. Done when: lifecycle test; approval-step-up test; revocation test.
6. **Hostile-plugin suite + example.** A suite of adversarial plugins (fs/network/over-scope/dep-vuln/unsigned) all denied+logged; a benign example plugin demonstrating a real capability. Done when: the hostile suite passes (all denied); example plugin works end-to-end within its capabilities.

## Concrete Steps
Runtime choice is load-bearing - Wasm/Wasmtime gives capability-based sandboxing that matches INV-6 cleanly (and the operator's prior work); confirm and Decision-Log. Default-deny everything; capabilities are explicit grants checked on every call, not ambient authority. The hostile-plugin suite is REQUIRED and comprehensive (it's the proof INV-6 holds). Approval uses EP-401 step-up. Run security-review.md every milestone. Commit per milestone.

## Validation and Acceptance
Per-milestone; `test-unit.sh` + `test-integration.sh` green; `verify.sh` + `security-check.sh` green; the hostile-plugin suite (escape/over-scope/unsigned/dep-vuln all denied+logged) is REQUIRED; per-call capability enforcement + approval-step-up tests REQUIRED; `git diff --name-only` matches. Acceptance: Phase-4 plugin exit - a generated plugin passes signing, sandbox, capability review, and hot-load; hostile plugins are contained.

## Idempotence and Recovery
Plugins are isolated - a misbehaving plugin can't affect the host (sandbox) and is revocable immediately; load is idempotent; the capability check runs every call so nothing is grandfathered. S9-adjacent: weakening the sandbox/signing/capability checks is a hard-deny (SECURITY.md HARD-DENY 7).

## Progress
- [x] M1 Runtime+sandbox  - [x] M2 Manifest+signing  - [x] M3 Capability host  - [x] M4 Dep-scan  - [x] M5 Lifecycle+approval  - [x] M6 Hostile suite+example

## Surprises & Discoveries
- The prior crate was a configuration registry, not a runtime; migration 0016 also made new rows approved by default. Migration 0042 moves legacy rows back to installed and requires durable approval evidence.
- Wasm modules do not expose a trustworthy language-package lockfile. The signed manifest therefore carries a deterministic SBOM plus lock hash, and the operator injects the current deny-policy database into the load gate.
- The workspace dependency audit also reports pre-existing `quick-xml` advisories through `rust-s3` and the unfixed RSA timing advisory; neither is introduced by this plan. Wasmi 0.40 resolves upstream's yanked-but-not-vulnerable `spin` 0.9.8.

## Decision Log
- 2026-07-18: Reopened after the EP-406 dependency audit proved the ledger's prior `done` status was unsupported: all milestones were unchecked, the crate had no executable Wasm sandbox or dependency gate, lifecycle was in-memory without step-up, and migration 0016 defaulted records directly to `approved`. EP-406 returns to draft until this gate is real.
- 2026-07-18: Use Wasmi 0.40 as the embedded interpreter. It supplies deterministic fuel metering, strict engine limits, and explicit host linking without linking WASI; this keeps filesystem and socket authority structurally absent while avoiding a JIT/native-code fast path.
- 2026-07-18: Approval receipts bind the exact signed manifest and exact capability grant. The Postgres loader locks the approved row while it revalidates the artifact and changes state, so no public state-transition method can bypass the gate.

## Outcomes & Retrospective
- `cargo test -p aether-plugin`: 13 passed, 1 Postgres test intentionally ignored in the default unit run; hostile fs/socket/unsigned/tampered/over-scope/vulnerable/fuel/memory cases are denied and audit-asserted, while the `read_markets.wat` example executes its granted host call.
- Isolated Postgres proof: migrations 1-42 applied from zero and the ignored durable lifecycle test passed (1/1); the scratch database was removed afterward.
- `cargo clippy -p aether-plugin --all-targets -- -D warnings`, `scripts/security-check.sh`, and the repository-wide `scripts/verify.sh` all pass. Live/restart integration was not run because the operator explicitly excluded restart-based live tests.
