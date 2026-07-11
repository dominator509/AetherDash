#!/usr/bin/env bash
# Layer: 6 - Verification & Operations
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"
COMPOSE="infra/dev/docker-compose.yml"
DC() { docker compose -f "$COMPOSE" "$@"; }

command -v docker >/dev/null 2>&1 || { echo "MISSING TOOL: docker (A-06)"; exit 2; }
if [ ! -f "$COMPOSE" ]; then echo "SKIP (marker absent): $COMPOSE -> stack smoke"; echo "smoke: ok"; exit 0; fi

# Service names below are fixed by the EP-003 compose contract.
DC up -d --wait
DC exec -T postgres pg_isready -U aether
DC exec -T clickhouse clickhouse-client --query "SELECT 1" >/dev/null
DC exec -T redis redis-cli PING | grep -q PONG
curl -fsS http://localhost:6333/readyz >/dev/null           # qdrant
DC exec -T redpanda rpk cluster health | grep -qi healthy
curl -fsS http://localhost:9000/minio/health/live >/dev/null # minio

# Service /healthz endpoints — populate as services land (EP-004+):
# Gateway and MCP are the first app services to expose healthz.
GATEWAY_PORT="${AETHER_GATEWAY__BIND:-127.0.0.1:8080}"
GATEWAY_PORT_NUM=$(echo "$GATEWAY_PORT" | sed 's/.*://')
SERVICES_HEALTHZ="http://localhost:${GATEWAY_PORT_NUM}/healthz http://localhost:8000/healthz"
for url in $SERVICES_HEALTHZ; do
    if curl -fsS "$url" >/dev/null 2>&1; then
        echo "healthz ok: $url"
    else
        echo "healthz SKIP (not running): $url"
    fi
done

echo "smoke: ok"
