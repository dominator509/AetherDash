Layer: 5 - Execution

# EP-002: Core Domain Types & Canonical Serialization

**Band:** 0xx Foundation | **Phase:** 0 | **Status:** active | **Blocked by:** EP-001

## Purpose / Big Picture
Implement SPEC-001 exactly: `crates/aether-core` as the type authority, proto mirrors as the wire authority, TS and Python mirrors proven equivalent by shared golden vectors. When this plan is done, "a Quote" means one thing in every plane and hashing/provenance has stable bytes to stand on.

## Scope
`crates/aether-core` (types, canonical serde, redis key consts, error envelope type, retry-policy config type); `proto/aether/core/v1/` message definitions (files only - codegen wiring is EP-004); `packages/types` (hand-mirrored TS per D7 comment rule); `pylib/aether_py` (mirrored Python models); shared golden vectors `testdata/golden/core/`.

## Non-goals
No IO, HTTP, or DB code in aether-core (D1); no tonic/build.rs (EP-004); no bus code; no venue anything.

## Context and Orientation
SPEC-001 is the whole contract - implement it, don't reinterpret it. The canonical-bytes requirement exists because SPEC-011 provenance hashes and EP-402 audit hashes are computed over them; instability here corrupts everything downstream.

## Files to Read First
1. SPEC-001 (entire).
2. SPEC-003 error envelope + SPEC-006 retry table (their types live here).
3. ARCHITECTURE.md D1/D7; SPEC-002 redis key prefixes.

## Files to Change (Expected Changed Files)
`Cargo.toml` (workspace member append), `crates/aether-core/**` (Cargo.toml, src/{lib,ids,time,decimal,market,quote,book,order,opportunity,edge,audit,caps,error,retry,redis_keys}.rs, tests/), `proto/aether/core/v1/{types,market_data,orders,opportunity}.proto`, `packages/types/**` (package.json, tsconfig, src/, tests/), `pylib/**` (pyproject append via uv workspace, aether_py/{__init__,models,canonical}.py, tests/), `pnpm-workspace.yaml`/root `pyproject.toml` member appends, `testdata/golden/core/*.json`, CHANGELOG Unreleased, this file.

## Interfaces and Contracts
Public API = SPEC-001 types verbatim; constructor-enforced invariants (OrderBook ordering, confidence 0..=1, EdgeDecomposition sum law checked in a `validate()` + debug_assert on construction); `canonical_json_bytes<T>()` in all three languages producing identical bytes (preserve declared field order, decimals as strings, RFC3339-millis, omit-none).

## Milestones
1. **Crate scaffold + scalars.** Ulid/MarketKey/Decimal-string/UTC-time modules with parse/format + property tests. Done when: `cargo nextest run -p aether-core` green (or `cargo test -p aether-core`).
2. **Market-data types.** Venue/InstrumentKind/Market/PriceSemantics/Quote/BookLevel/OrderBook + constructor invariants. Done when: unit tests incl. ordering rejection green.
3. **Order & risk types.** Money/OrderIntent/RiskVerdict/Order/Fill/Position/CapsSnapshot + closed reason enum. Done when: green.
4. **Opportunity types.** OpportunityKind/Opportunity/EdgeDecomposition/BrainRef/AuditEvent + sum-law property test (proptest: random components -> validate holds iff sum holds; explicit-zero law: serde rejects missing components). Done when: green.
5. **Golden vectors.** Write `testdata/golden/core/<type>.json` arrays of {name, value} from Rust (a `cargo run -p aether-core --bin gen-goldens --features golden-gen` generator, feature-gated so D1 stays clean); hand-review values (esp. 18-decimal, negative, zero-explicit cases per SPEC-001). Done when: Rust round-trip test over every vector green.
6. **Proto mirrors.** The four .proto files mirroring the types (decimal fields as string, Timestamp for ts) with `// mirrors: aether-core::<Type>` comments (D7). Done when: `protoc --proto_path=proto --descriptor_set_out=/dev/null proto/aether/core/v1/*.proto` succeeds if protoc present, else `buf lint` if present, else syntax-review + Decision Log note deferring compile-proof to EP-004.
7. **TS mirror.** `packages/types` with the same types (decimals as string, branded types for Ulid/MarketKey), canonical stringify, vitest suite consuming the SAME golden files (`import` via relative path or copy script - choose copy-at-test-time to keep the package publishable; log the choice). Done when: `pnpm --filter @aether/types test -- --run` green.
8. **Python mirror.** `aether_py.models` (pydantic v2, Decimal fields, model_config for canonical dump) + canonical.py; pytest over the same goldens. Done when: `uv run pytest pylib -q` green.

## Concrete Steps
Dependencies to add (Decision-Log each): aether-core: serde, serde_json (preserve_order feature), rust_decimal, ulid, time (or chrono - pick `time`, log it), thiserror, proptest(dev). TS: none runtime (types + hand stringify), vitest(dev). Python: pydantic>=2, (dev) pytest. Follow the append pattern from EP-001 for workspace members. Keep each milestone a commit.

## Validation and Acceptance
Per-milestone commands above; plan-level: all three test suites green via `scripts/test-unit.sh`; cross-language byte-equality test (a test in each language hashing its canonical bytes for every golden and comparing to `sha256` values stored IN the golden file - generated by M5, so TS/Py prove equality to Rust); grep audit `rg -n 'f64|as f32' crates/aether-core/src` -> no money/price/size hits; `verify.sh` -> `verify: ok`; Cargo.lock now committed (closes EP-001's note).

## Idempotence and Recovery
Golden regeneration is explicit (`--features golden-gen`) and diff-reviewed - a changed golden is a canonical-bytes break and needs a Decision Log entry + downstream hash-impact note (none yet at this stage, which is why goldens land NOW). Type additions later follow SPEC-001's amendment rule.

## Progress
- [x] M1 Scalars  - [x] M2 Market data  - [x] M3 Orders  - [x] M4 Opportunity
- [x] M5 Goldens  - [x] M6 Proto  - [x] M7 TS  - [x] M8 Python

## Surprises & Discoveries
- **serde_json preserve_order**: critical for canonical bytes. Without it, key ordering is non-deterministic. All three languages must agree on field order (Rust struct declaration order is canonical). Python's canonical serialization preserves dict insertion order (Python 3.7+) to match.
- **time crate 0.3.53**: uses i128 for nanos. All conversions from i64 millis need `as i128` cast. `unix_timestamp_nanos()` returns i128, so `unix_millis()` casts back to i64.
- **ruff 0.15.x**: changed `[tool.ruff.format]` schema. `line-length` is no longer valid; use `docstring-code-line-length` instead.
- **mypy + pydantic**: pydantic stubs must be installed via `uv sync` for mypy to find them. Without `pydantic>=2` in dev deps, mypy reports "Cannot find implementation" for BaseModel.
- **uv workspace install**: `uv sync` removes editable installs not in the workspace. Pylib must be in `members = ["pylib"]` for persistence, but `uv pip install -e ./pylib` works for dev iteration.
- **Cross-language SHA-256**: Python's `json.dumps` with `sort_keys=True` produces different canonical bytes than Rust's struct-field-declaration order. Removing `sort_keys` and using Python 3.7+ insertion-order preservation fixes it — golden SHA-256 values now match across all three languages.

## Decision Log
- **DL-002-1**: Crate dependency `time` (not `chrono`) for UtcTime per EP-002 Concrete Steps.
- **DL-002-2**: `sha2` + `hex` added to aether-core for canonical_sha256(). These are pure-computation crates (no IO, HTTP, DB) — consistent with D1.
- **DL-002-3**: TS golden test reads golden JSON files from `testdata/golden/core/` via relative path. Copy-at-test-time considered but adds complexity; direct read chosen for v1.
- **DL-002-4**: Python uses `json.dumps(ensure_ascii=True, separators=(",", ":"))` without `sort_keys` to match Rust struct-field declaration order. This is the cross-language canonical contract.
- **DL-002-5**: `gen-goldens` binary is feature-gated (`golden_gen`) so D1 stays clean — no filesystem IO in core lib without opt-in.
- **DL-002-6**: `pydantic>=2` added to root dev deps so mypy can type-check models.py. Not a runtime dependency of aether-core.
- **DL-002-7**: Proto `side_exposure` changed from `double` to `string` (SPEC-001 no-float rule). Position in Rust uses `#[serde(with = "decimal_string")]`.

## Outcomes & Retrospective
- **Rust**: 66 unit tests + 4 golden integration tests + 1 proptest = 71 passing
- **TypeScript**: `@aether/types` package — 2 golden vector tests, tsc noEmit clean, vitest green
- **Python**: `aether_py` package — 2 golden vector tests, mypy clean, ruff clean
- **Cross-language**: SHA-256 matches across Rust, TypeScript, and Python for all 12 golden vectors
- **Golden files**: money.json (4), edge.json (3), confidence.json (3), market_key.json (2) — all with SHA-256
- **Proto**: 4 files with all SPEC-001 types, closed enums, no floats
- **verify.sh**: `verify: ok` — all three stacks exercise with real tests
- **security-check.sh**: `security: ok`
- **git log**: 3 EP-002 commits (initial + progress update + completion)
