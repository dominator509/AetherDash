#!/usr/bin/env bash
# Layer: 6 - Verification & Operations
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"
skip() { echo "SKIP (marker absent): $*"; }

if [ -f Cargo.toml ]; then cargo fetch; else skip "Cargo.toml -> rust install"; fi
if [ -f pnpm-workspace.yaml ]; then
  if [ -f pnpm-lock.yaml ]; then pnpm install --frozen-lockfile
  else echo "WARN: pnpm-lock.yaml missing; running unfrozen install - commit the lockfile (ADR-0005)"; pnpm install; fi
else skip "pnpm-workspace.yaml -> ts install"; fi
if [ -f pyproject.toml ]; then uv sync; else skip "pyproject.toml -> python install"; fi

echo "install: ok"
