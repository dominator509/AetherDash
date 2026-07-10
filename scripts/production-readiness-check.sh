#!/usr/bin/env bash
# Layer: 6 - Verification & Operations
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

"$ROOT/scripts/verify.sh"
"$ROOT/scripts/test-integration.sh"
"$ROOT/scripts/test-e2e.sh"
"$ROOT/scripts/security-check.sh"
"$ROOT/scripts/dependency-audit.sh"
"$ROOT/scripts/smoke-test.sh"

cd "$ROOT"
PRD="aether-blueprint/PRODUCTION_READINESS.md"
[ -f "$PRD" ] || { echo "FAIL: $PRD missing"; exit 1; }
# Convention: required checklist items are lines of the form "- [ ] REQUIRED: ..." / "- [x] REQUIRED: ...".
if grep -Fq -- "- [ ] REQUIRED" "$PRD"; then
  echo "FAIL: unchecked REQUIRED items in $PRD:"
  grep -Fn -- "- [ ] REQUIRED" "$PRD"
  exit 1
fi

echo "production readiness: ok"
