#!/usr/bin/env bash
# Layer: 6 - Verification & Operations
# EP-408: Production readiness gate.
# Runs every verification tier, then parses PRODUCTION_READINESS.md for
# unchecked REQUIRED items.  Exits 0 only if all pass and checklist is green.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"
FAIL=0

banner()  { echo ""; echo "================================================"; echo "  $*"; echo "================================================"; }
pass()    { echo "PASS: $*"; }
fail()    { echo "FAIL: $*"; FAIL=1; }

# ---------------------------------------------------------------------------
# 1. Full verification chain (the gate from AGENTS.md section 14)
# ---------------------------------------------------------------------------
banner "verify.sh"
if sh "$ROOT/scripts/verify.sh"; then
  pass "verify.sh"
else
  fail "verify.sh"
fi

# ---------------------------------------------------------------------------
# 2. Integration tests (requires dev compose stack; EP-003)
# ---------------------------------------------------------------------------
banner "test-integration.sh"
if sh "$ROOT/scripts/test-integration.sh"; then
  pass "test-integration.sh"
else
  fail "test-integration.sh"
fi

# ---------------------------------------------------------------------------
# 3. E2E tests (Playwright; active after EP-101)
# ---------------------------------------------------------------------------
banner "test-e2e.sh"
if sh "$ROOT/scripts/test-e2e.sh"; then
  pass "test-e2e.sh"
else
  fail "test-e2e.sh"
fi

# ---------------------------------------------------------------------------
# 4. Security check -- secret scan, forbidden paths, boundary D3
# ---------------------------------------------------------------------------
banner "security-check.sh"
if sh "$ROOT/scripts/security-check.sh"; then
  pass "security-check.sh"
else
  fail "security-check.sh"
fi

# ---------------------------------------------------------------------------
# 5. Dependency audit -- cargo-audit / pnpm audit / pip-audit
# ---------------------------------------------------------------------------
banner "dependency-audit.sh"
if sh "$ROOT/scripts/dependency-audit.sh"; then
  pass "dependency-audit.sh"
else
  fail "dependency-audit.sh"
fi

# ---------------------------------------------------------------------------
# 6. Smoke test -- dev stack health endpoints
# ---------------------------------------------------------------------------
banner "smoke-test.sh"
if sh "$ROOT/scripts/smoke-test.sh"; then
  pass "smoke-test.sh"
else
  fail "smoke-test.sh"
fi

# ---------------------------------------------------------------------------
# 7. Health check -- all service /healthz endpoints
# ---------------------------------------------------------------------------
banner "health-check.sh"
if sh "$ROOT/scripts/health-check.sh"; then
  pass "health-check.sh"
else
  fail "health-check.sh"
fi

# ---------------------------------------------------------------------------
# 8. Parse PRODUCTION_READINESS.md for unchecked REQUIRED items
# ---------------------------------------------------------------------------
banner "PRODUCTION_READINESS.md checklist audit"
PR_FILE="$ROOT/aether-blueprint/PRODUCTION_READINESS.md"
if [ ! -f "$PR_FILE" ]; then
  fail "Missing $PR_FILE"
else
  UNCHECKED=$(grep -c '^- \[ \] REQUIRED' "$PR_FILE" || true)
  if [ "$UNCHECKED" -gt 0 ]; then
    fail "$UNCHECKED unchecked REQUIRED item(s) in PRODUCTION_READINESS.md:"
    grep -n '^- \[ \] REQUIRED' "$PR_FILE" | sed 's/^/  /'
  else
    pass "All REQUIRED checklist items checked"
  fi
fi

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
echo ""
if [ "$FAIL" -eq 0 ]; then
  echo "production readiness: ok"
  exit 0
else
  echo "production readiness: FAILED"
  exit 1
fi
