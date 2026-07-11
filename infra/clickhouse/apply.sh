#!/usr/bin/env bash
# SPEC-002: ClickHouse DDL applier -- idempotent, ordered replay
set -euo pipefail

CH_URL="${AETHER_CLICKHOUSE__URL:-http://localhost:8123}"
CH_DB="${AETHER_CLICKHOUSE__DATABASE:-aether}"
CH_USER="${AETHER_CLICKHOUSE__USER:-aether}"
CH_PASS="${AETHER_CLICKHOUSE__PASSWORD:-aether}"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Create database if not exists
curl -fsS -u "${CH_USER}:${CH_PASS}" "${CH_URL}" \
    --data-binary "CREATE DATABASE IF NOT EXISTS ${CH_DB}" >/dev/null

echo "Applying ClickHouse DDL to ${CH_URL} database=${CH_DB} ..."
for f in "$SCRIPT_DIR"/*.sql; do
    if [ -f "$f" ]; then
        echo "  $(basename "$f")"
        # Strip `--` comments, collapse newlines, split on `;`.
        # Process substitution avoids a subshell so curl failures propagate.
        while IFS= read -r stmt; do
            stmt="$(echo "$stmt" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')"
            if [ -n "$stmt" ] && [ "$stmt" != ";" ]; then
                curl -fsS -u "${CH_USER}:${CH_PASS}" "${CH_URL}" \
                    --data-binary "${stmt}" >/dev/null
            fi
        done < <(sed '/^--/d' "$f" | tr '\n' ' ' | sed 's/;/;\n/g')
    fi
done
echo "ClickHouse DDL complete."
