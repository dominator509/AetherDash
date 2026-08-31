#!/usr/bin/env bash
# Layer: 6 - Verification & Operations
# The gate referenced by "Definition of done" (AGENTS.md section 14).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

bash "$ROOT/scripts/preflight.sh"
bash "$ROOT/scripts/format-check.sh"
bash "$ROOT/scripts/lint.sh"
bash "$ROOT/scripts/typecheck.sh"
bash "$ROOT/scripts/test-unit.sh"
bash "$ROOT/scripts/build.sh"

echo "verify: ok"
