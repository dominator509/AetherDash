#!/usr/bin/env bash
# Layer: 6 - Verification & Operations
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"
COMPOSE="infra/dev/docker-compose.yml"

command -v docker >/dev/null 2>&1 || { echo "MISSING TOOL: docker (A-06)"; exit 2; }
if [ ! -f "$COMPOSE" ]; then echo "SKIP (marker absent): $COMPOSE -> integration tests"; echo "integration: ok"; exit 0; fi

docker compose -f "$COMPOSE" up -d --wait

# Convention (AGENTS.md section 10): Rust integration tests are #[ignore]-tagged.
if [ -f Cargo.toml ]; then cargo test --workspace -- --ignored; fi

if [ -f pyproject.toml ]; then
  set +e
  uv run pytest -m integration -q
  rc=$?
  set -e
  if [ "$rc" -ne 0 ] && [ "$rc" -ne 5 ]; then exit "$rc"; fi
  [ "$rc" -eq 5 ] && echo "NOTE: no python integration tests collected yet"
fi

echo "integration: ok"
