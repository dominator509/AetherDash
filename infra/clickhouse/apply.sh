#!/usr/bin/env bash
# SPEC-002: ClickHouse DDL applier -- idempotent, ordered replay
set -euo pipefail

CH_URL="${AETHER_CLICKHOUSE__URL:-http://localhost:8123}"
CH_DB="${AETHER_CLICKHOUSE__DATABASE:-aether}"
CH_AUTH="${AETHER_CLICKHOUSE__USER:-aether}:${AETHER_CLICKHOUSE__PASSWORD:-aether}"
CURL="curl -fsS -u \"$CH_AUTH\" \"$CH_URL\""

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Create database if not exists
$CURL --data-binary "CREATE DATABASE IF NOT EXISTS ${CH_DB}" >/dev/null 2>&1

echo "Applying ClickHouse DDL to ${CH_URL} database=${CH_DB} ..."
for f in "$SCRIPT_DIR"/*.sql; do
    if [ -f "$f" ]; then
        echo "  $(basename "$f")"
        $CURL --data-binary "@${f}" >/dev/null
    fi
done
echo "ClickHouse DDL complete."
