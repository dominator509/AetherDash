#!/usr/bin/env bash
# Layer: 6 - Verification & Operations
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"
skip() { echo "SKIP (marker absent): $*"; }

if [ -f Cargo.toml ]; then cargo fmt --all --check; else skip "Cargo.toml -> rustfmt"; fi
if [ -f pnpm-workspace.yaml ]; then pnpm -r --if-present run format:check; else skip "pnpm-workspace.yaml -> prettier"; fi
if [ -f pyproject.toml ]; then uv run ruff format --check .; else skip "pyproject.toml -> ruff format"; fi

echo "format: ok"
