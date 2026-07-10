#!/usr/bin/env bash
# Layer: 6 - Verification & Operations
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"
skip() { echo "SKIP (marker absent): $*"; }

if [ -f pnpm-workspace.yaml ]; then pnpm -r --if-present run typecheck; else skip "pnpm-workspace.yaml -> tsc"; fi
if [ -f pyproject.toml ]; then
  DIRS=""
  for d in server pylib; do
    if [ -d "$d" ] && find "$d" -name "*.py" -print -quit 2>/dev/null | grep -q .; then
      DIRS="$DIRS $d"
    fi
  done
  DIRS="$(echo "$DIRS" | xargs)"  # trim
  if [ -n "$DIRS" ]; then uv run mypy $DIRS; else skip "no python source files yet -> mypy"; fi
else skip "pyproject.toml -> mypy"; fi
# Rust type checking is covered by clippy/build (COMMANDS.md).

echo "typecheck: ok"
