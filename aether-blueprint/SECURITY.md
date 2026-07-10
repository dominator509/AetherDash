Layer: 3 - Architecture

# SECURITY.md - Threat Model and Binding Rules

ARCHITECTURE.md section 7 defines the boundaries; this file defines the threats, the rules, and the checks. Anything here marked HARD-DENY is enforced at all permission tiers and may never be weakened by an agent (STOP S9).

## Threat model (v1, single operator)
| # | Threat | Primary defenses |
|---|---|---|
| T1 | Network attacker reaching server plane | WireGuard-only admin access; gateway token auth; TLS on public webhooks; no service binds public except gateway + webhook receivers |
| T2 | Malicious/corrupt venue data | Schema validation at adapter boundary before bus publish; quarantine topic for rejects; price-sanity bands in risk engine |
| T3 | Prompt injection via ingested content (email, PDFs, web) | All Brain/web content is DATA, never instructions; tool allowlist enforced server-side by tier regardless of model output; INV-1 keeps LLMs off the execution path entirely; tier >= 3 actions require deterministic confirmation flows |
| T4 | Key/fund theft | Wallet Guardian isolation (INV-5); propose-only API; human approval for withdrawals; per-tx and daily limits; allowlisted destinations; HARD-DENY hooks below |
| T5 | Supply-chain compromise | Committed lockfiles (ADR-0005); `scripts/dependency-audit.sh`; D1-D7 keeps blast radius bounded; execution-plane dep additions need ADR-style notes (AGENTS.md 8) |
| T6 | Malicious or buggy plugin | Signed manifests, sandbox, capability checks at host boundary on every call, dependency scan at load (INV-6) |
| T7 | Agent error / misaligned automation | STOP S7-S9; expected-changed-files gate; `execution.live_enabled` unreachable to agents (ADR-0007); audit chain |
| T8 | Operator error | Step-up 2FA for irreversible actions; first-live-trade ceremony (OPERATIONS.md); caps enforced server-side |

## Secret handling (binding)
- Secrets never appear in: the repository, logs, test output, recordings, prompts/model context, MCP tool results, error messages, or agent final responses.
- Storage: dev = `infra/dev/.env.dev` (gitignored) + OS keychain for the client; prod = systemd `LoadCredential`/EnvironmentFile outside the repo (A-16). `.env.example` files carry names and dummy values only.
- Agents never read or echo secret values (EXECUTION_RULES R12). Needing a secret's value is STOP S1.
- `scripts/security-check.sh` enforces: no forbidden tracked files (`.env`, `*.pem`, `*.key`, `id_*`), no secret-shaped strings, no D3 boundary violations. It must pass for any Definition of done.

## HARD-DENY inventory (all tiers, including YOLO tier 5)
1. Reading/exporting wallet private keys, seeds, or keystore files by any component except the Guardian's keystore module.
2. Any code path placing key material, `.env` contents, or credential values into model context or logs.
3. Setting `execution.live_enabled` by agent, config-templating, or test fixture; it is operator-edited out-of-band only (ADR-0007).
4. Raising, removing, or bypassing user-defined caps programmatically.
5. Disabling audit-chain writes, log redaction, permission checks, or geofencing.
6. Wallet withdrawals/transfers above the policy threshold without fresh human approval + step-up 2FA.
7. Loading an unsigned or capability-over-scoped plugin.

## Trust boundaries and input validation
- **Adapter boundary:** every inbound venue payload validates against the pack's schema; failures go to `quarantine.<venue>` with the raw payload preserved in MinIO, never onto `md.*`.
- **Inbox boundary:** attachments parse in a resource-limited worker (no macro execution, no external fetches during parse); extracted text enters the Brain flagged `origin=inbox, trust=low` until source-reliability scoring (EP-206) says otherwise.
- **Gateway boundary:** every WS message authenticates (token) and authorizes (tier) before dispatch; malformed frames drop with a counter metric, no reflection of content into logs.
- **Plugin boundary:** capability check on every host call; filesystem/network denied by default.
- **MCP boundary:** tool inventory is tier-filtered server-side (SPEC-003); the model never sees tools its tier cannot call.

## AuthN/Z summary (full spec: SPEC-005)
Single operator v1: local account, argon2id password hash, TOTP for step-up. Five permission tiers (Read-Only, Draft-Only, Confirm-Every-Action, Bounded-Autopilot, YOLO-within-hard-caps) attach to sessions and to each agent/automation identity. Tier grants live in Postgres (`permission_grants`) and are evaluated server-side at gateway, router, and MCP layers; the client renders but never decides.

## Security testing
- `security-check.sh` in every verify chain that gates done-ness of security-relevant plans (EP-305/306/401/403 acceptance runs it explicitly).
- Risk-engine rejection tests per TESTING.md execution-path minimums.
- Guardian policy tests: limits, allowlists, approval expiry, replay of a stale approval must fail.
- Plugin sandbox escape attempts: a hostile fixture plugin trying fs/network/over-scope calls must be denied and logged.

## Incident response
Runbooks (from `runbook-template.md`) cover: suspected key exposure (freeze Guardian, rotate, audit chain review), venue credential leak (revoke, rotate, review orders), poisoned ingest (quarantine source, purge derived objects by provenance hash). OPERATIONS.md owns the operational steps; this file owns the requirement that those runbooks exist before Phase 2 exit.
