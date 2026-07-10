#!/usr/bin/env bash
# Layer: 6 - Verification & Operations
# The gate referenced by "Definition of done" (AGENTS.md section 14).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

"$ROOT/scripts/preflight.sh"
"$ROOT/scripts/format-check.sh"
"$ROOT/scripts/lint.sh"
"$ROOT/scripts/typecheck.sh"
"$ROOT/scripts/test-unit.sh"
"$ROOT/scripts/build.sh"

echo "verify: ok"
