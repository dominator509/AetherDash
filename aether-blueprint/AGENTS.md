Layer: 1 - Governance

# AGENTS.md - Control Plane for Coding Agents

This file governs every coding agent operating in this repository. Read it fully before any edit.

## 1. Mission
Implement AETHER Terminal exactly as specified by the active ExecPlan, keeping every load-bearing invariant intact (ARCHITECTURE.md section "Invariants"). Work autonomously, validate constantly, stop only for the STOP conditions below.

## 2. Source-of-truth priority
When instructions conflict, higher entries win:
1. Current user instruction (in-session)
2. AGENTS.md (this file)
3. The active ExecPlan (`.agent/PLANS.md` names it)
4. Existing repository code and tests
5. ARCHITECTURE.md
6. The relevant spec in `.agent/specs/`
7. ROADMAP.md

If obeying a lower level would violate a higher level, follow the higher level and record the conflict in the ExecPlan Decision Log.

## 3. Required workflow
1. Read AGENTS.md (this file).
2. Read COMMANDS.md.
3. Read `.agent/PLANS.md`; identify the single active ExecPlan; read it fully.
4. Run `scripts/preflight.sh`.
5. Complete the ExecPlan milestones in order.
6. After every milestone: run that milestone's validation commands; update the ExecPlan Progress section.
7. Continue autonomously to the next milestone.
8. Stop only under a STOP condition.
9. At completion: run `scripts/verify.sh`, run `git diff --name-only`, update Outcomes & Retrospective, produce the final response (section 15).

**Do not ask the user for next steps. Proceed autonomously through the active ExecPlan unless a STOP condition applies.**

## 4. STOP conditions
Stop, report, and wait for a human decision when ANY of the following applies:
- **S1** A required secret, credential, paid service, or external account is missing (venue API keys, LLM API keys, Twilio, RPC endpoints, brokerage accounts).
- **S2** An action may destroy user or production data (dropping tables, deleting the vault, irreversible migration on non-dev data).
- **S3** A legal, security, or financial judgment is required that specs do not already resolve (jurisdiction eligibility, licensing, AGPL exposure).
- **S4** A materially different user-visible behavior choice is not resolved by the spec.
- **S5** Required tests cannot run after the documented recovery ladder (section 7) has been exhausted.
- **S6** Production deployment or an irreversible migration without explicit permission.
- **S7 (AETHER)** Any change that could submit a live (non-paper) order, enable live-trading configuration, raise or remove a user-defined cap, or alter geofencing/venue-eligibility logic.
- **S8 (AETHER)** Any change touching wallet key material, signer code paths, Wallet Guardian policy, or key-custody configuration.
- **S9 (AETHER)** Any change that disables, bypasses, or weakens: hard-deny hooks, the five-tier permission model, the audit chain, or log redaction.

When stopping, report: the exact blocker, evidence (file paths / terminal output), the smallest decision needed, and a recommended default.

**Refusal (not merely stop):** requests to add anti-bot circumvention (CAPTCHA/Cloudflare/Turnstile bypass, stealth fingerprinting, evasion proxying) are a load-bearing non-goal. Do not implement them in any form; point to PROJECT_BRIEF.md "Out of scope".

## 5. Anti-drift rules
- Implement only what the active ExecPlan scopes. Non-goals are binding.
- No broad refactors, styling rewrites, dependency swaps, file reorganizations, or unrelated cleanup unless the active ExecPlan explicitly requires them.
- Every ExecPlan lists Expected Changed Files. Final review compares `git diff --name-only` against that list; any extra file must be justified in the Decision Log.
- Never edit files outside the active ExecPlan's plane band (EP-1xx = `client/`, EP-2xx = `server/`, EP-3xx = `connectors/`, EP-0xx/EP-4xx as scoped) except shared files the plan names.
- Do not implement from ROADMAP.md directly.

## 6. Anti-hallucination rules
- Do not invent package APIs, command names, environment variables, database tables, routes, config keys, gRPC methods, or bus topics.
- Confirm names by reading repository files (`Cargo.toml`, `package.json`, `pyproject.toml`, `proto/`, `infra/migrations/`, ENVIRONMENT.md) before use.
- Commands come from COMMANDS.md only. If a needed command is missing or stale, update COMMANDS.md from repository evidence first, then use it.
- Before adding a dependency: check existing dependencies; check whether existing tools suffice; add only if necessary; record the decision; update install/build docs.
- Record every assumption in the ExecPlan Decision Log and, if durable, in ASSUMPTIONS.md.

## 7. Anti-fixation rules (bounded retry)
For any failing validation command:
1. **First failure:** read the error, identify the likely cause, make the smallest targeted fix.
2. **Second same-root failure:** create or run a narrower diagnostic (single test, single crate, verbose flag); isolate; no broad rewrites.
3. **Third same-root failure:** stop that approach, record failed hypotheses in Surprises & Discoveries, choose a simpler implementation path consistent with the spec, continue if safe.
Never patch blindly around the same error indefinitely. If no simpler path exists, S5 applies.

## 8. Dependency rules
- Respect the dependency direction rules in ARCHITECTURE.md ("Import and dependency rules"). Violations fail review even if tests pass.
- Execution-plane crates (`connectors/execution/*`) may depend only on `crates/aether-core` and vetted infrastructure crates; never on LLM clients, MCP code, or anything under `server/`.
- Pin versions via lockfiles (Cargo.lock, pnpm-lock.yaml, uv.lock); lockfiles are always committed.
- New third-party dependencies in execution-plane or wallet code additionally require a Decision Log entry naming the crate, version, and why no existing tool sufficed.

## 9. File creation rules
- New files go where ARCHITECTURE.md's repo map places them; when unclear, mirror the nearest existing sibling.
- Every new Rust module gets unit tests in-file or in `tests/`; every new TS component gets a vitest file; every new Python module gets a pytest file.
- Generated artifacts (proto stubs, sqlx offline data, vault views) are never hand-edited; regenerate via COMMANDS.md commands.
- No file may embed secrets, keys, tokens, or live endpoints. Config templates use `*.example` suffixes.

## 10. Testing rules
- TESTING.md defines the strategy; this section binds behavior.
- Every milestone's validation commands must pass before the next milestone starts.
- Integration tests follow the repository convention: Rust `#[ignore]`-tagged tests run via `scripts/test-integration.sh`; Python `-m integration` marker; both require the dev compose stack.
- Trading correctness changes (router, risk engine, simulator, Wallet Guardian, connectors) require: unit tests, replay-based integration tests against recorded fixtures, and an updated lifecycle assertion where the opportunity lifecycle is affected.
- Never delete or weaken a failing test to make validation pass; fix the code or STOP (S5).

## 11. Documentation update rules
- Behavior changes update the relevant spec in the same ExecPlan.
- New commands update COMMANDS.md; new env vars update ENVIRONMENT.md; new decisions append to DECISIONS.md via the ADR template.
- ExecPlan Progress, Surprises & Discoveries, and Decision Log are updated as work happens, not retroactively at the end.

## 12. Security rules
- Never commit secrets; never print secret values to logs, test output, or the final response. `scripts/security-check.sh` must pass before completion.
- Never read or echo the contents of `.env`, key files, or keychain entries. Hard-deny: any tooling change that would place key material into model context or prompt-construction code paths.
- All external input (venue payloads, inbox attachments, plugin manifests) is validated at the trust boundary per SPEC-006.
- Log statements in new code must route through the redaction layer once it exists (EP-404); until then, never log request bodies or headers from authenticated calls.

## 13. Production data rules
- Development uses the compose stack and recorded fixtures only. No agent connects to production databases, live wallets, or live-money venue endpoints.
- Paper-trading endpoints and venue sandboxes are permitted where the ExecPlan names them.
- Any migration is written to be reversible (paired down-migration) unless the ExecPlan explicitly accepts irreversibility - which triggers S6 on production data.

## 14. Definition of done
An ExecPlan is done only when ALL hold:
- Every acceptance criterion in the ExecPlan passes.
- All required validation commands pass (exact commands, exact expected outputs).
- ExecPlan Progress is current; Outcomes & Retrospective written.
- Final diff reviewed: only Expected Changed Files changed, extras justified.
- Remaining risks documented in the ExecPlan.
- `scripts/verify.sh` prints `verify: ok`.

## 15. Final response requirements
The final response for any ExecPlan must include, in order:
1. ExecPlan completed (ID + title).
2. Changed files (from `git diff --name-only`).
3. Commands run and their results (pass/fail, key output lines).
4. Acceptance criteria status, one line each.
5. Decisions made (with Decision Log references).
6. Assumptions confirmed or changed (with ASSUMPTIONS.md references).
7. Remaining risks.
8. Whether production-readiness criteria are affected and their current status.
