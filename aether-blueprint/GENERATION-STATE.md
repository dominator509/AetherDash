# GENERATION-STATE.md

Manifest required by the master prompt's GENERATION SESSION PROTOCOL.

**Master prompt:** `AETHER-MasterPrompt-v2.md` (patched).
**Pack version:** Session 5 - COMPLETE, 2026-07-09.
**Status: ALL FILES COMPLETE. The blueprint pack is fully generated.**

## Invariant ledger (holds across the pack)
INV-1 AI pilot not engine; INV-2 three planes, MCP off trading path; INV-3 cache-first prompting; INV-4 compliance-first ingestion, no anti-bot; INV-5 wallet isolated behind Guardian, human-approved withdrawals; INV-6 plugins signed/sandboxed/scoped; INV-7 venues additive, zero core edits; INV-8 Simple/Advanced one engine; INV-9 DBs truth, vault generated, provenance+staleness; INV-10 self-improvement metric-driven+human-gated; INV-11 router/risk/guardian/connectors separate+tested, paper/backtest first-class.

## File manifest - all complete
- **Layer 1 Governance:** AGENTS.md, .agent/EXECUTION_RULES.md, .agent/prompts/ (4).
- **Layer 2 Product/Decisions:** PROJECT_BRIEF, ASSUMPTIONS, ROADMAP, DECISIONS, CONTRIBUTING, adr-template.
- **Layer 3 Architecture:** ARCHITECTURE, SECURITY, ENVIRONMENT.
- **Layer 4 Specification:** spec-template + SPEC-000..SPEC-012 (14 files).
- **Layer 5 Execution:** .agent/PLANS.md (32-plan ledger, all `draft`), execplan-template, and all 32 ExecPlan bodies (EP-000..004, EP-101..104, EP-201..207, EP-301..308, EP-401..408).
- **Layer 6 Verification/Operations:** COMMANDS, scripts/ (14), test-case-template, runbook-template, TESTING, DEPLOYMENT, OPERATIONS, OBSERVABILITY, PRODUCTION_READINESS, RELEASE, ROLLBACK, all 9 checklists, USAGE.md.

Total: 98 files. Every .md declares its Layer. All scripts syntax-valid. No encoding artifacts.

## Session log
- **S1 (2026-07-07):** Patched prompt -> v2; 29 pack files (7 root docs, ledger, EXECUTION_RULES, 4 prompts, 5 templates, 14 scripts). Conventions: marker-gated scripts; REQUIRED checkbox; compose names; sqlx sole authority; live_enabled hard-deny.
- **S2 (2026-07-07):** 10 ops/arch docs + SPEC-000..003. Conventions: env/port map (CH native 9004); decimal-string wire, ULID, MarketKey, EdgeDecomposition sum law; store names/retention; proto packages, bus envelope, WS frame table, tiered MCP, closed error codes; HARD-DENY numbering; /opt/aether layout; metric registry.
- **S3 (2026-07-09):** SPEC-004..012 + EP-000..004 bodies. Conventions: tiers+step-up+caps-lower-of-two; fail-closed/open + no-auto-retry-on-submit + breaker params; four pillars + redaction.toml + trace_id law; gate evidence rules; venue.toml + INV-7 diff check; Guardian policy order + withdrawal-always-human; object kinds/pipeline/tiering/RRF/vault-exclusions; lifecycle table + 11-component net-edge + mismatch.toml + shared fill model; workspace-member append; aether-proto crate + golden cross-language sha256.
- **S4 (2026-07-09):** EP-101..104 + EP-201..207 bodies + 9 checklists. Seams documented for stub replacement (EP-305/307/202 client seams, brain router-stub, alerts paper-only, inbox OCR park).
- **S5 (2026-07-09):** EP-301..308 + EP-401..408 bodies + USAGE.md. Ledger fully `draft`. Integrity pass clean (layer headers, script syntax, encoding). PACK COMPLETE.

## Executing the pack (not generating it)
See USAGE.md. Set EP-000 `active` and use `.agent/prompts/execute-active-execplan.md`. The generation protocol above is meta (producing the blueprint); USAGE.md governs building the product from it.
