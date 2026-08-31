#!/usr/bin/env bash
# Layer: 6 - Verification & Operations
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"
skip() { echo "SKIP (marker absent): $*"; }

if [ -f pnpm-workspace.yaml ]; then
  # Tauri's Rust context macro validates `frontendDist` during `cargo build`.
  # Materialize the Vite output first so clean CI checkouts and local verifies
  # follow the same frontend-before-Rust build order.
  if [ -f client/vite.config.ts ] && [ -f client/package.json ]; then
    pnpm --filter @aether/client exec vite build
  else
    skip "client/vite.config.ts -> vite build"
  fi
else
  skip "pnpm-workspace.yaml -> vite build"
fi
if [ -f Cargo.toml ]; then cargo build --workspace; else skip "Cargo.toml -> cargo build"; fi
if [ -f pnpm-workspace.yaml ]; then pnpm -r --if-present run build; else skip "pnpm-workspace.yaml -> ts build"; fi
if [ -f pyproject.toml ]; then
  DIRS=""
  [ -d server ] && DIRS="$DIRS server"
  [ -d pylib ] && DIRS="$DIRS pylib"
  if [ -n "$DIRS" ]; then uv run python -m compileall -q $DIRS; else skip "no python source dirs yet -> compileall"; fi
else skip "pyproject.toml -> compileall"; fi

echo "build: ok"
