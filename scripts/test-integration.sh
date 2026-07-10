#!/usr/bin/env bash
# Layer: 6 - Verification & Operations
# EP-003: Integration tests — brings up dev stack, runs bus-independent SPEC-002 tests
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"
COMPOSE="infra/dev/docker-compose.yml"

command -v docker >/dev/null 2>&1 || { echo "MISSING TOOL: docker (A-06)"; exit 2; }
if [ ! -f "$COMPOSE" ]; then echo "SKIP (marker absent): $COMPOSE -> integration tests"; echo "integration: ok"; exit 0; fi

echo "=== Starting dev stack ==="
docker compose -f "$COMPOSE" up -d --wait

# Convention (AGENTS.md section 10, TESTING.md): Rust integration tests are #[ignore]-tagged.
echo "=== Running Rust integration tests ==="
if [ -f Cargo.toml ]; then
    cargo test --workspace -- --ignored --test-threads=1
else
    echo "SKIP (marker absent): Cargo.toml -> Rust integration tests"
fi

echo "=== Running Python integration tests ==="
if [ -f pyproject.toml ]; then
  set +e
  uv run pytest -m integration -q
  rc=$?
  set -e
  if [ "$rc" -ne 0 ] && [ "$rc" -ne 5 ]; then echo "FAIL: Python integration tests"; exit "$rc"; fi
  [ "$rc" -eq 5 ] && echo "NOTE: no python integration tests collected yet"
else
    echo "SKIP (marker absent): pyproject.toml -> Python integration tests"
fi

echo "integration: ok"
