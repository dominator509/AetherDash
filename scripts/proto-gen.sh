#!/usr/bin/env bash
# Layer: 4 - Proto Code Generation
# Compiles proto contracts for all target languages.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"

# Detect Python and uv
PYTHON=""
if command -v python3 >/dev/null 2>&1; then PYTHON=python3
elif command -v python >/dev/null 2>&1; then PYTHON=python
else echo "MISSING TOOL: python3/python"; exit 2; fi

UV=""
if command -v uv >/dev/null 2>&1; then UV="uv run --frozen"
fi

RED='\033[0;31m'; GREEN='\033[0;32m'; NC='\033[0m'
PASS=0; FAIL=0
ok()   { PASS=$((PASS+1)); echo -e "  ${GREEN}PASS${NC} $1"; }
fail() { FAIL=$((FAIL+1)); echo -e "  ${RED}FAIL${NC} $1"; }

# ── 1. Rust proto codegen (via build.rs / tonic-build) ─────────────────────
echo "---"
echo "Rust: cargo build -p aether-proto"
if cargo build -p aether-proto 2>&1; then
  ok "aether-proto build"
else
  fail "aether-proto build"
fi

# ── 2. Golden vector regeneration ──────────────────────────────────────────
echo "---"
echo "Rust: cargo run -p aether-core --features golden-gen --bin gen-goldens"
if cargo run -p aether-core --features golden-gen --bin gen-goldens 2>&1; then
  ok "golden vectors regenerated"
else
  fail "golden vectors regeneration"
fi

# ── 3. Python golden tests ─────────────────────────────────────────────────
echo "---"
echo "Python: golden tests"
if $UV $PYTHON -m pytest pylib/aether_py/tests/test_goldens.py -q 2>&1; then
  ok "Python golden tests"
else
  fail "Python golden tests"
fi

# ── 4. Python cross-language canonical tests ───────────────────────────────
echo "---"
echo "Python: cross-language canonical tests"
if $UV $PYTHON -m pytest pylib/aether_py/tests/test_cross_language_canonical.py -q 2>&1; then
  ok "Python cross-language canonical tests"
else
  fail "Python cross-language canonical tests"
fi

# ── 5. TypeScript golden tests ─────────────────────────────────────────────
echo "---"
echo "TypeScript: golden tests"
if cd packages/types && pnpm test -- --run 2>&1; then
  ok "TypeScript golden tests"
  cd "$ROOT"
else
  cd "$ROOT"
  fail "TypeScript golden tests"
fi

# ── 6. Cross-language type-name coverage check ─────────────────────────────
echo "---"
echo "Cross-language: type-name coverage check"
if bash scripts/proto-type-coverage-check.sh 2>&1; then
  ok "cross-language type-name coverage check"
else
  fail "cross-language type-name coverage check"
fi

# ── Summary ────────────────────────────────────────────────────────────────
echo "---"
echo "proto-gen: ${PASS} pass, ${FAIL} fail, $((PASS+FAIL)) total"
if [ "$FAIL" -gt 0 ]; then
  exit 1
fi
echo "proto-gen: ok"
