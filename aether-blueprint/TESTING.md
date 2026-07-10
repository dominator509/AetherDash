Layer: 6 - Verification & Operations

# TESTING.md - Test Strategy and Binding Conventions

Commands live in COMMANDS.md; this file defines what tests exist, where they live, and what agents may never do to them.

## Philosophy
AETHER moves money. The test suite's job is to make the deterministic core (INV-1) provably deterministic and the AI layer safely fallible. Ranked priorities: (1) execution-path correctness (router, risk, guardian, connectors, net-edge math), (2) data integrity (canonical serialization, provenance, audit chain), (3) behavior of everything else.

## Test types and locations
| Type | Where | Convention | Run via |
|---|---|---|---|
| Rust unit | in-file `#[cfg(test)] mod tests` | pure, no IO, no network | `scripts/test-unit.sh` |
| Rust integration | crate `tests/` | `#[ignore]`-tagged; needs compose stack | `scripts/test-integration.sh` |
| TS unit/component | colocated `*.test.ts(x)` (vitest) | jsdom; no network | `scripts/test-unit.sh` |
| Client e2e | `client/e2e/` (Playwright) | runs against `tauri dev` build | `scripts/test-e2e.sh` |
| Python unit | package `tests/`, no marker | pure | `scripts/test-unit.sh` |
| Python integration | marker `@pytest.mark.integration` | needs compose stack | `scripts/test-integration.sh` |
| Replay | `tests/replay/` per consumer crate + `testdata/` | see below | `scripts/test-integration.sh` |
| Golden | alongside unit tests, fixtures in `testdata/golden/` | byte-exact expected outputs | `scripts/test-unit.sh` |

Markers `e2e` and `integration` are the only pytest markers with runtime meaning; register both in pyproject.

## Fixtures and recorded data
- Venue payload recordings live in `testdata/<venue>/` as scrubbed JSON/NDJSON (no account IDs, no keys - scrubbing is part of the recording script each venue pack ships).
- Recordings are committed; they are the contract with reality. Refreshing a recording is a Decision Log event in the plan doing it.
- Deterministic clock: all time-dependent logic accepts an injected clock; tests never sleep to pass.

## Replay harness (first-class, INV-11; built in EP-405, used from EP-301 on)
The harness replays recorded venue streams onto the bus (`md.ticks.*`, `md.books.*`) with original relative timing (or compressed), then asserts downstream outputs: normalized quotes, detected opportunities, simulator numbers, router decisions. A replay test names its recording, its consumers, and byte-exact or tolerance-bounded expectations. Determinism rule: same recording -> same `opps.detected` sequence, always.

## Lifecycle assertions (INV attribution requirement)
Integration suites assert every `Opportunity` reaches a terminal state: detected -> scored -> surfaced -> (accepted|ignored|expired) -> (executed|not) -> outcome -> attributed. An opportunity event chain left open at suite teardown is a failure. Implemented as a shared checker consuming `opportunity_events` (SPEC-002).

## Net-edge math (SPEC-012)
Every `EdgeDecomposition` component has golden tests with hand-computed expected values, plus property tests: components sum to `net_edge`; decimals round-trip; no component silently defaults to zero when its input is present.

## Execution-path minimums (binding per AGENTS.md section 10)
Any change to router, risk engine, simulator, Wallet Guardian, or a venue adapter requires: unit tests for the changed logic, a replay test exercising it against recordings, and an updated lifecycle assertion if the opportunity flow changed. Risk engine: every rejection reason (liveness, price drift, balance, venue health, caps, jurisdiction) has at least one test proving it fires and one proving it doesn't misfire.

## LLM-dependent code
LLM calls are mocked at the `llm_router` boundary with canned responses in unit tests; no test may hit a paid API. The router itself has contract tests against a local stub server. Prompt-assembly is pure and golden-tested (cache-prefix stability is an assertable property: identical static blocks -> identical prefix bytes, INV-3).

## What agents may never do
- Delete, skip, `#[ignore]`-untag-then-retag, weaken assertions, or widen tolerances to make a failing test pass (EXECUTION_RULES R12).
- Add sleeps to fix flakiness; fix the race or inject the clock.
- Commit recordings containing secrets or account identifiers.
- Reduce a REQUIRED test listed in a spec's Required Tests section to RECOMMENDED.

## Traceability
Every spec's "Required behavior" items map to named tests in its "Required tests" section. `final-review.md` check 6 audits this mapping for the touched specs.

## Coverage stance
No blanket percentage gate in v1. The gates are: spec traceability, execution-path minimums, replay determinism, lifecycle closure. If a coverage tool is added later it is informational until an ADR says otherwise.
