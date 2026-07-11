#!/usr/bin/env bash
# Layer: 4 - Proto Code Generation
# Compiles proto contracts for all target languages.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

echo "Proto codegen: Rust via build.rs (automatic)"
echo "Proto codegen: Python — hand-mirrored per D7 (pylib/aether_py/proto/__init__.py)"
echo "Proto codegen: TypeScript — hand-mirrored per D7 (packages/types/src/proto/index.ts)"
echo "proto-gen: ok"
