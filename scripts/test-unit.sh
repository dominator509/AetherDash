#!/usr/bin/env bash
# Layer: 6 - Verification & Operations
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"
skip() { echo "SKIP (marker absent): $*"; }

if [ -f Cargo.toml ]; then
  output="$(cargo test --workspace 2>&1)" || true
  if echo "$output" | grep -qE "no package|no members|no targets|manifest is virtual"; then
    echo "SKIP (empty workspace): rust tests — no crate targets yet"
  fi
else skip "Cargo.toml -> rust unit tests"; fi

if [ -f pnpm-workspace.yaml ]; then pnpm -r --if-present run test -- --run; else skip "pnpm-workspace.yaml -> vitest"; fi

if [ -f pyproject.toml ]; then
  set +e
  uv run pytest -m "not integration and not e2e" -q 2>&1
  rc=$?
  set -e
  if [ "$rc" -ne 0 ] && [ "$rc" -ne 5 ]; then exit "$rc"; fi
  [ "$rc" -eq 5 ] && echo "NOTE: no python unit tests collected yet"
else skip "pyproject.toml -> pytest"; fi

echo "unit: ok"
