#!/usr/bin/env bash
# Layer: 6 - Verification & Operations
# The gate referenced by "Definition of done" (AGENTS.md section 14).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

bash "$ROOT/scripts/preflight.sh"
bash "$ROOT/scripts/format-check.sh"

# Tauri's Rust context macro validates `frontendDist` during every Rust
# compile, including Clippy and unit-test builds. Generate the Vite bundle
# before those gates run on a clean checkout.
if [ -f "$ROOT/pnpm-workspace.yaml" ] && [ -f "$ROOT/client/vite.config.ts" ] && [ -f "$ROOT/client/package.json" ]; then
  (cd "$ROOT" && pnpm --filter @aether/client exec vite build)
fi

bash "$ROOT/scripts/lint.sh"
bash "$ROOT/scripts/typecheck.sh"
bash "$ROOT/scripts/test-unit.sh"
bash "$ROOT/scripts/build.sh"

echo "verify: ok"
