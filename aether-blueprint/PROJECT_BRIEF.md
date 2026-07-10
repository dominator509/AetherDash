Layer: 2 - Product & Decisions

# PROJECT_BRIEF - AETHER Terminal

## Project name
AETHER Terminal. The name is a placeholder identity; renaming it has no architectural impact.

## Problem statement
Market intelligence and execution for prediction markets, equities/options, crypto/DeFi, and sports-event markets is fragmented across venue UIs, data terminals, chat alerts, and spreadsheets. Cross-venue edges (pricing gaps, catalysts, mispriced events) decay before a human can gather context, compute true net edge after all costs, and execute safely. Existing terminals are closed, expensive, single-asset-class, and not AI-native. AETHER unifies the four asset classes into one self-hosted terminal where a deterministic engine owns data, risk, and execution, and AI owns understanding, research, and natural-language control.

## Target users
- **Primary operator:** a technically sophisticated, self-hosting power user living in Advanced mode (full order books, cross-venue arbitrage, strategy builder, command room, plugin authoring, wallet controls, data provenance).
- **Prosumer / retail users (later):** operate safely in Simple mode - AI-curated opportunity feed, plain-English rationale, simulate, one-tap execution within caps - without understanding venues, bridges, oracles, or CLOBs.
- Design bias: single-operator first; architected for multi-user with RBAC later. All personas share one engine.

## Primary user outcomes
1. See AI-detected opportunities across all four asset classes as one feed.
2. Understand any opportunity in plain English, expandable down to raw evidence.
3. Simulate any trade and see net-edge decomposition (gross spread minus fees, slippage, funding, gas, bridge cost, settlement-mismatch discount, liquidity haircut, staleness and confidence penalties).
4. Execute safely through a deterministic order router and, for wallets, a separate Wallet Guardian.
5. Receive actionable alerts with inline Execute/Simulate/Ignore via Telegram, Discord, Slack, SMS, email.
6. Feed the Brain by forwarding email, PDFs, filings, and screenshots to a dedicated inbox.
7. Drive everything via natural language and slash commands under an explicit permission tier.
8. Launch bounded research swarms that return a single decision packet with citations.
9. Generate, test, and hot-load sandboxed plugins with signed, human-approved capability manifests.
10. Operate at all times within hard user-defined caps.

## Business goals
- Maximize expected risk-adjusted edge after all costs for the operator; never promise guaranteed profit.
- Ruthless cost efficiency: 90%+ LLM prefix cache-hit target, local-model fallback for high-frequency low-value calls.
- Self-hosted to minimize recurring cost and maximize privacy/control.
- Single-operator first to keep compliance and licensing surface small; AGPL exposure (OpenBB-derived services) flagged for legal review before any multi-user exposure.

## Technical goals
- AI is a pilot, not the engine: ingestion, normalization, risk checks, order validation, wallet policy, and audit are deterministic and never delegated to an LLM.
- Strict three-plane separation: client plane (Tauri), server/brain plane (24/7 VPS + GPU), connector plane (stateless microservices). MCP is the control plane only and never sits on the low-latency trading path.
- Orders fire on API venues in ~20-50 ms; arbitrage scan cadence ~500 ms; Brain retrieval ~100 ms; risk engine blocks fast.
- Databases are the source of truth; the Obsidian-compatible Markdown vault is a generated view.
- Every market connector is an additive extension pack; adding a venue requires no core changes.
- Simple and Advanced modes share one engine and one dataset.

## Out of scope (deliberate, load-bearing)
- Any anti-bot circumvention (CAPTCHA/Cloudflare/Turnstile bypass, stealth fingerprinting, evasion proxying).
- Forking or reusing Bloomberg's closed terminal; OpenBB is the TradFi data analog.
- LLM custody of raw private keys, or LLM-as-execution-engine.
- Unbounded autonomy; guaranteed-profit claims or features implying them.
- Polymarket execution for US users (geofenced).
- Multi-user/team/enterprise features, FIX adapters, institutional custody in v1.
- iMessage as core infrastructure.
- The Brain silently rewriting its own rules.

## Success metrics
- Realized vs predicted edge convergence over time; alert precision and false-positive rate trending up/down respectively.
- LLM prefix cache-hit rate >= 90% in steady state; inference cost per surfaced opportunity trending down.
- Zero incidents of: secret in logs/commits, order without router validation, wallet action without Guardian approval, live trade above caps.
- Opportunity lifecycle fully attributed (detected -> scored -> shown -> accepted/ignored -> executed/not -> outcome -> P&L -> reason logged) for 100% of opportunities.
- Slippage / fill quality within simulator-predicted bands.

## Production readiness definition
AETHER v1 is production-ready when: all Phase 1-4 core outcomes work end-to-end against live data with paper trading plus small-cap live execution; `scripts/verify.sh` and `scripts/production-readiness-check.sh` pass; the PRODUCTION_READINESS.md checklist passes in full (functional, testing, security, privacy, performance, accessibility, observability, deployment, operations); the audit chain verifies; and all eleven load-bearing invariants (see ARCHITECTURE.md) hold under test.
