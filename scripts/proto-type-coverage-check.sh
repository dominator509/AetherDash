#!/usr/bin/env bash
# Layer: 4 - Proto Code Generation
# Cross-language proto type-name coverage check: verify all service .proto type
# names have corresponding definitions in Python and TypeScript mirrors.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"

PYTHON=""
if command -v python3 >/dev/null 2>&1; then PYTHON=python3
elif command -v python >/dev/null 2>&1; then PYTHON=python
else echo "MISSING TOOL: python3/python"; exit 2; fi

UV=""
if command -v uv >/dev/null 2>&1; then UV="uv run --frozen"
fi

echo "Cross-language type-name coverage check:"
# The descriptor check script reads files only (no aether_py imports),
# so plain python works without uv. But uv is used if available for consistency.
$UV $PYTHON "$ROOT/scripts/proto-type-coverage-check.py"

echo "proto-type-coverage-check: ok"
