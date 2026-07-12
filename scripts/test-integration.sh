#!/usr/bin/env bash
# Layer: 6 - Verification & Operations
# EP-003: Integration tests — brings up dev stack, runs bus-independent SPEC-002 tests
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"
COMPOSE="infra/dev/docker-compose.yml"

command -v docker >/dev/null 2>&1 || { echo "MISSING TOOL: docker (A-06)"; exit 2; }
if [ ! -f "$COMPOSE" ]; then echo "FAIL (required infrastructure absent): $COMPOSE — integration tests cannot run without the compose stack"; exit 1; fi

# Check sqlx-cli is installed (needed for migration pairing tests)
if ! cargo sqlx --version >/dev/null 2>&1; then
    echo "MISSING TOOL: cargo-sqlx (install with: cargo install sqlx-cli)"
    exit 2
fi

echo "=== Starting dev stack ==="
docker compose -f "$COMPOSE" up -d --wait

# Wait for Postgres to accept connections
echo "Waiting for Postgres..."
until docker compose -f "$COMPOSE" exec -T postgres pg_isready -U aether >/dev/null 2>&1; do
    sleep 1
done

# Convention (AGENTS.md section 10, TESTING.md): Rust integration tests are #[ignore]-tagged.
# Live tests require these markers; without them the tests skip silently.
echo "=== Running Rust integration tests ==="
if [ -f Cargo.toml ]; then
    export AETHER_INTEGRATION_TEST=1
    export AETHER_REDPANDA_TEST=1
    export AETHER_KAFKA_BOOTSTRAP="${AETHER_KAFKA_BOOTSTRAP:-localhost:9092}"
    export DATABASE_URL="${DATABASE_URL:-postgres://aether:aether@localhost:5432/aether}"
    echo "  AETHER_INTEGRATION_TEST=1"
    echo "  AETHER_REDPANDA_TEST=1"
    echo "  AETHER_KAFKA_BOOTSTRAP=${AETHER_KAFKA_BOOTSTRAP}"
    echo "  DATABASE_URL=${DATABASE_URL}"
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
