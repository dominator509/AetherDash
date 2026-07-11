#!/usr/bin/env bash
# Layer: 4 - Proto Code Generation
# Compiles proto contracts for all target languages.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

echo "Proto codegen: Rust via build.rs (automatic)"
echo "Proto codegen: Python — SKIP (hand-mirrored per D7, see pylib/aether_py/models.py)"
echo "Proto codegen: TypeScript — SKIP (hand-mirrored per D7, see packages/types/src/index.ts)"
echo "proto-gen: ok"
