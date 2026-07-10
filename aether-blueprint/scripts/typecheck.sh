#!/usr/bin/env bash
# Layer: 6 - Verification & Operations
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"
skip() { echo "SKIP (marker absent): $*"; }

if [ -f pnpm-workspace.yaml ]; then pnpm -r --if-present run typecheck; else skip "pnpm-workspace.yaml -> tsc"; fi
if [ -f pyproject.toml ]; then
  DIRS=""
  [ -d server ] && DIRS="$DIRS server"
  [ -d pylib ] && DIRS="$DIRS pylib"
  if [ -n "$DIRS" ]; then uv run mypy $DIRS; else skip "no python source dirs yet -> mypy"; fi
else skip "pyproject.toml -> mypy"; fi
# Rust type checking is covered by clippy/build (COMMANDS.md).

echo "typecheck: ok"
