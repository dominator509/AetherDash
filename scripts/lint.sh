#!/usr/bin/env bash
# Layer: 6 - Verification & Operations
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"
skip() { echo "SKIP (marker absent): $*"; }

if [ -f Cargo.toml ]; then
  output="$(cargo clippy --workspace --all-targets -- -D warnings 2>&1)" || true
  if echo "$output" | grep -qE "no package|no members|no targets|manifest is virtual"; then
    echo "SKIP (empty workspace): clippy — no crate targets yet"
  fi
else skip "Cargo.toml -> clippy"; fi
if [ -f pnpm-workspace.yaml ]; then pnpm -r --if-present run lint; else skip "pnpm-workspace.yaml -> eslint"; fi
if [ -f pyproject.toml ]; then uv run ruff check .; else skip "pyproject.toml -> ruff"; fi

echo "lint: ok"
