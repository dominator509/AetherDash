#!/usr/bin/env bash
# Layer: 6 - Verification & Operations
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"

# ACTIVE after EP-101 creates the client package (COMMANDS.md).
if [ ! -d client ] || [ ! -f pnpm-workspace.yaml ]; then
  echo "SKIP (marker absent): client/ -> playwright e2e"; echo "e2e: ok"; exit 0
fi
pnpm --filter @aether/client run --if-present e2e
echo "e2e: ok"
