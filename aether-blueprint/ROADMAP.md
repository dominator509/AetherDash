Layer: 2 - Product & Decisions

# ROADMAP - AETHER Terminal

**Do not implement directly from this file. Implementation must happen through an ExecPlan.** This roadmap sequences phases and points to specs and plans; it contains no instructions precise enough to code against.

The authoritative plan ledger (IDs, bands, status, blocking) is `.agent/PLANS.md`. Phase timing is indicative (~16 weeks total); exit criteria are not.

## Phase 0 - Repository discovery and foundation
**Purpose:** an empty repo becomes a verifiable three-stack monorepo with the shared substrate every plane depends on.
**Plans:** EP-000, EP-001, EP-002, EP-003, EP-004. **Specs:** SPEC-001 (core domain), SPEC-002 (data model), SPEC-003 (service contracts).
**Dependencies:** none.
**Exit criteria:** `scripts/verify.sh` prints `verify: ok` exercising all three stacks (no SKIPs for rust/ts/py); `scripts/smoke-test.sh` passes against the dev compose stack; `aether-core` types round-trip canonically across Rust/TS/Python with tests; bus topic registry and proto contracts compile.

## Phase 1 - Terminal core
**Purpose:** the operator sees real cross-venue data in a real client, with a Brain, alerts, and paper trading - no live money.
**Plans:** client EP-101, EP-102, EP-103; brain EP-201, EP-202, EP-203, EP-204; connectors EP-301 (Kalshi, reference pack), EP-302 (Polymarket read-only), EP-303 (Hyperliquid read + OpenBB + Alpaca paper), EP-304 (paper trading ledger). **Specs:** SPEC-004 (UI/UX behavior), SPEC-009 (venue connector contract), SPEC-011 (brain objects & recall), SPEC-012 (opportunity lifecycle).
**Dependencies:** Phase 0 complete.
**Exit criteria:** opportunity feed renders live normalized data from >= 3 venues; every opportunity carries a plain-English explain view expandable to evidence; paper trades record fills and P&L in the ledger; alerts with inline Simulate/Ignore arrive on Telegram, Discord, and Slack; forwarded email lands as Brain objects with provenance; LLM cache-hit metric is exported.

## Phase 2 - Execution
**Purpose:** small live trades through deterministic gates.
**Plans:** EP-305 (order router + risk engine), EP-306 (Wallet Guardian + WalletConnect v2), EP-307 (arb scanner + trade simulator), EP-308 (SMS/email + approval flows), EP-104 (advanced panels/DOM), EP-401 (five-tier permissions + 2FA + hard-deny hooks), EP-402 (audit chain + P&L attribution), EP-404 (observability baseline). **Specs:** SPEC-005 (auth & permissions), SPEC-006 (error handling), SPEC-007 (observability), SPEC-010 (wallet guardian policy).
**Dependencies:** Phase 1 exit; operator-provided credentials (STOP S1 items).
**Exit criteria:** router blocks each failure class (liveness, price drift, balance, venue health, caps, jurisdiction) under test; a small live trade executes on one venue within caps and appears in the verified audit chain; wallet actions require Guardian approval end-to-end; scanner runs at ~500 ms cadence with cost-aware filtering; simulator's net-edge decomposition matches SPEC-012 math on fixtures.

## Phase 3 - Brain and ingestion
**Purpose:** the Brain gets deep: compliant ingestion at scale, richer retrieval, reliability scoring.
**Plans:** EP-206 (ingestion fleet + OCR + source reliability), EP-207 (tiered recall v2), EP-405 (testing hardening + replay harness expansion). **Specs:** updates to SPEC-011.
**Dependencies:** Phase 1 exit; GPU worker decision (A-15).
**Exit criteria:** ingestion honors the compliance ladder with per-source audit of which rung was used; retrieval meets the ~100 ms budget on the benchmark set; news-to-move attribution produces scored links; deterministic replay reproduces recorded market days bit-identically into the scanner.

## Phase 4 - Agents and plugins
**Purpose:** the command room becomes an operating system: swarms, plugins, self-improvement loop.
**Plans:** EP-205 (swarms + decision packets), EP-403 (plugin runtime), EP-406 (code-writing agent + cron + backtesting agent), EP-407 (deployment & release engineering), EP-408 (production readiness closure). **Specs:** SPEC-008 (production readiness), plugin/capability additions to SPEC-005.
**Dependencies:** Phase 2 exit (permissions + audit must precede autonomous surfaces).
**Exit criteria:** a swarm returns one decision packet with citations under budget; a generated plugin passes manifest signing, sandbox, capability review, and hot-load; self-improvement proposals arrive as human-gated diffs with backtest evidence (INV-10); `scripts/production-readiness-check.sh` prints `production readiness: ok`.

## Phase 5 - Pro/enterprise (explicitly out of v1)
Multi-user + RBAC, compliance exports, FIX-style adapters, data entitlements, institutional custody. No plans exist yet by design; opening this phase requires revisiting ADR-0001 (repo shape), A-17 (CI), and the AGPL flag in PROJECT_BRIEF.md.

## Production readiness milestone
Reached at Phase 4 exit per PROJECT_BRIEF.md "Production readiness definition" and PRODUCTION_READINESS.md. Phases 1-4 each end with a demo-able, restartable system; no phase leaves the repo in a state where `verify.sh` fails.
