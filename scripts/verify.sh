#!/usr/bin/env bash
# Layer: 6 - Verification & Operations
# The gate referenced by "Definition of done" (AGENTS.md section 14).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

sh "$ROOT/scripts/preflight.sh"
sh "$ROOT/scripts/format-check.sh"
sh "$ROOT/scripts/lint.sh"
sh "$ROOT/scripts/typecheck.sh"
sh "$ROOT/scripts/test-unit.sh"
sh "$ROOT/scripts/build.sh"

echo "verify: ok"
