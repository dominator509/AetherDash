Layer: 5 - Execution

# .agent/PLANS.md - The ExecPlan Ledger

This file is the single authority on which plans exist, their status, and which one is active. Agents: exactly ONE plan is `active` at any time; implement only that plan.

## Lifecycle
`pending` (contents not yet generated) -> `draft` (contents exist, not started) -> `active` (being implemented; max one) -> `done` (Definition of done met, AGENTS.md section 14) -> or `revise` (reopened with a Decision Log reason). Status changes are edits to this table plus the plan's own Progress section.

## Active plan
**EP-201** — Brain v1: object model, provenance, recall v1, vault view

## Ledger
| ID | Title | Band | Phase | Status | Blocked by |
|----|-------|------|-------|--------|------------|
| EP-000 | Repository discovery & pack installation check | 0xx Foundation | 0 | done | - |
| EP-001 | Monorepo scaffold, toolchains, CI skeleton | 0xx Foundation | 0 | done | EP-000 |
| EP-002 | Core domain types & canonical serialization | 0xx Foundation | 0 | done | EP-001 |
| EP-003 | Data & persistence substrate (compose stack, migrations, DDL) | 0xx Foundation | 0 | done | EP-002 |
| EP-004 | Service contracts & event bus (proto, topics, WS gateway skeleton) | 0xx Foundation | 0 | done | EP-003 |
| EP-101 | Tauri shell: toggle, keyboard nav, command line, encrypted cache | 1xx Client | 1 | done | EP-004 |
| EP-102 | Opportunity feed & explain views | 1xx Client | 1 | draft | EP-101, EP-304 |
| EP-103 | Command room harness (MCP client, slash commands, tier surface) | 1xx Client | 1 | draft | EP-101, EP-202 |
| EP-104 | Advanced panels: undockable layout, order books / DOM | 1xx Client | 2 | draft | EP-102 |
| EP-201 | Brain v1: object model, provenance, recall v1, vault view | 2xx Brain | 1 | done | EP-003 |
| EP-202 | LLM router, cache-first prompting, local fallback | 2xx Brain | 1 | draft | EP-001 |
| EP-203 | Alert engine & comms (Telegram, Discord, Slack, inline actions) | 2xx Brain | 1 | draft | EP-004 |
| EP-204 | Agentic inbox (Gmail push, MS Graph, parse/scan/file) | 2xx Brain | 1 | draft | EP-201 |
| EP-205 | Research swarms & decision packets | 2xx Brain | 4 | draft | EP-103, EP-202 |
| EP-206 | Ingestion fleet, OCR, source-reliability scoring | 2xx Brain | 3 | draft | EP-201 |
| EP-207 | Tiered recall v2: hybrid fusion, graph traversal, decay, rerank | 2xx Brain | 3 | draft | EP-201 |
| EP-301 | Venue pack: Kalshi (reference implementation + replay fixtures) | 3xx Connectors | 1 | draft | EP-004 |
| EP-302 | Venue pack: Polymarket read-only (CLOB, Gamma, Polygon RPC) | 3xx Connectors | 1 | draft | EP-301 |
| EP-303 | Venue packs: Hyperliquid read, OpenBB foundation, Alpaca paper | 3xx Connectors | 1 | draft | EP-301 |
| EP-304 | Paper trading ledger & fill recording | 3xx Connectors | 1 | draft | EP-301 |
| EP-305 | Order router & risk engine (paper-first, then small live) | 3xx Connectors | 2 | draft | EP-304, EP-401 |
| EP-306 | Wallet Guardian & WalletConnect v2 | 3xx Connectors | 2 | draft | EP-401 |
| EP-307 | Arbitrage scanner & trade simulator (net-edge math) | 3xx Connectors | 2 | draft | EP-302, EP-303, EP-304 |
| EP-308 | Comms expansion: Twilio SMS, email, approval flows | 3xx Connectors | 2 | draft | EP-203 |
| EP-401 | Five-tier permissions, step-up 2FA, hard-deny hooks | 4xx Cross-cutting | 2 | draft | EP-004 |
| EP-402 | Audit chain end-to-end & P&L attribution | 4xx Cross-cutting | 2 | draft | EP-305 |
| EP-403 | Plugin runtime: signed manifests, sandbox, capability host | 4xx Cross-cutting | 4 | draft | EP-401 |
| EP-404 | Observability: redaction, Prometheus, health, self-improvement metrics | 4xx Cross-cutting | 2 | draft | EP-004 |
| EP-405 | Testing hardening: replay harness, lifecycle assertions, regression | 4xx Cross-cutting | 3 | draft | EP-305, EP-307 |
| EP-406 | Code-writing agent, cron jobs, backtesting agent | 4xx Cross-cutting | 4 | draft | EP-403 |
| EP-407 | Deployment & release engineering (plane hosts, systemd, wiring) | 4xx Cross-cutting | 4 | draft | EP-404 |
| EP-408 | Production readiness closure | 4xx Cross-cutting | 4 | draft | all above |

## Minimum-coverage mapping (master prompt EP-000..EP-010)
| Minimum plan | Covered by |
|---|---|
| EP-000 repository discovery | EP-000 |
| EP-001 foundation | EP-001 |
| EP-002 core domain | EP-002 |
| EP-003 data & persistence | EP-003 |
| EP-004 api/service layer | EP-004, EP-203 |
| EP-005 UI/client | EP-101, EP-102, EP-103, EP-104 |
| EP-006 auth, security, permissions | EP-401, EP-306, EP-403 |
| EP-007 testing hardening | EP-405 |
| EP-008 observability & operations | EP-404 |
| EP-009 deployment & release | EP-407 |
| EP-010 production readiness | EP-408 |

## Rules restated (binding)
- One venue = one ExecPlan; adding a venue touches only the EP-3xx band plus registry/spec/plan files (INV-7 check).
- Plans stay small: 6-10 milestones. If a plan grows past that, split it here first.
- Do not implement from ROADMAP.md; it points here.
- Activating a plan whose Blocked-by entries are not all `done` requires a Decision Log justification in the plan.
