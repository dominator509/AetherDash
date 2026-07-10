#!/usr/bin/env bash
# Layer: 6 - Verification & Operations
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"
skip() { echo "SKIP (marker absent): $*"; }

if [ -f Cargo.toml ]; then
  command -v cargo-audit >/dev/null 2>&1 || { echo "MISSING TOOL: cargo-audit (cargo install cargo-audit)"; exit 2; }
  cargo audit
else skip "Cargo.toml -> cargo audit"; fi

if [ -f pnpm-workspace.yaml ] && [ -f pnpm-lock.yaml ]; then
  pnpm audit --prod --audit-level high
else skip "pnpm lockfile -> pnpm audit"; fi

if [ -f pyproject.toml ]; then
  if uv run python -c 'import pip_audit' >/dev/null 2>&1; then uv run pip-audit
  else echo "SKIP: pip-audit not installed (add as dev dependency to enable)"; fi
else skip "pyproject.toml -> pip-audit"; fi

echo "audit: ok"
