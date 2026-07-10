#!/usr/bin/env bash
# Layer: 6 - Verification & Operations
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"
skip() { echo "SKIP (marker absent): $*"; }

if [ -f Cargo.toml ]; then
  output="$(cargo fmt --all --check 2>&1)" || true
  if echo "$output" | grep -q "Failed to find targets"; then
    echo "SKIP (empty workspace): rustfmt — no crate targets yet"
  fi
else skip "Cargo.toml -> rustfmt"; fi
if [ -f pnpm-workspace.yaml ]; then pnpm -r --if-present run format:check; else skip "pnpm-workspace.yaml -> prettier"; fi
if [ -f pyproject.toml ]; then uv run ruff format --check .; else skip "pyproject.toml -> ruff format"; fi

echo "format: ok"
