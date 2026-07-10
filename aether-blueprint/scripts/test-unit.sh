#!/usr/bin/env bash
# Layer: 6 - Verification & Operations
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"
skip() { echo "SKIP (marker absent): $*"; }

if [ -f Cargo.toml ]; then
  if command -v cargo-nextest >/dev/null 2>&1; then cargo nextest run --workspace; else cargo test --workspace; fi
else skip "Cargo.toml -> rust unit tests"; fi

if [ -f pnpm-workspace.yaml ]; then pnpm -r --if-present run test -- --run; else skip "pnpm-workspace.yaml -> vitest"; fi

if [ -f pyproject.toml ]; then
  set +e
  uv run pytest -m "not integration and not e2e" -q
  rc=$?
  set -e
  # pytest exit 5 = no tests collected: legal while packages are still landing
  if [ "$rc" -ne 0 ] && [ "$rc" -ne 5 ]; then exit "$rc"; fi
  [ "$rc" -eq 5 ] && echo "NOTE: no python unit tests collected yet"
else skip "pyproject.toml -> pytest"; fi

echo "unit: ok"
