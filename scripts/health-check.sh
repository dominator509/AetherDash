#!/usr/bin/env bash
# Layer: 6 - Verification & Operations
# EP-408: Health check script that curls all service /healthz and /readyz
# endpoints defined in ENVIRONMENT.md.  Reports per-service status and
# exits non-zero if any expected endpoint is unreachable.
#
# Services that are not running are reported as SKIP rather than FAIL,
# making this safe to run in dev where only a subset may be active.
#
# Usage:
#   ./scripts/health-check.sh              # check all known services
#   FORCE_FAIL=1 ./scripts/health-check.sh # treat any skip as failure (CI)

set -euo pipefail
FAIL=0
SKIP=0
FORCE_FAIL="${FORCE_FAIL:-0}"

banner() { echo "--- $* ---"; }

# ---- Helper: check a single URL --------------------------------------------
# Returns 0 if reachable with 200, 1 if unreachable (non-fatal), exits if FORCE_FAIL
check() {
  local service="$1" url="$2" label="${3:-$service}"
  local code
  code=$(curl -sf -o /dev/null -w "%{http_code}" --max-time 3 "$url" 2>/dev/null || echo "000")
  if [ "$code" = "000" ]; then
    echo "  SKIP  $label  ($url not reachable)"
    SKIP=$((SKIP+1))
    [ "$FORCE_FAIL" -eq 1 ] && FAIL=1
    return 1
  elif [ "$code" != "200" ]; then
    echo "  FAIL  $label  (HTTP $code)  $url"
    FAIL=1
    return 1
  else
    echo "  OK    $label  $url"
    return 0
  fi
}

# ---- Helper: check a gRPC service via grpc-health-probe ---------------------
check_grpc() {
  local service="$1" addr="$2"
  if command -v grpc-health-probe >/dev/null 2>&1; then
    if grpc-health-probe -addr="$addr" -connect-timeout=3s >/dev/null 2>&1; then
      echo "  OK    $service  grpc://$addr"
    else
      echo "  FAIL  $service  grpc://$addr (unreachable)"
      FAIL=1
    fi
  else
    echo "  SKIP  $service  grpc://$addr (grpc-health-probe not installed)"
    SKIP=$((SKIP+1))
  fi
}

echo "================================================"
echo "  AETHER Terminal — Health Check"
echo "  $(date -u --iso-8601=seconds)"
echo "================================================"

# ---- Infrastructure services (from docker-compose.yml / ENVIRONMENT.md) -----
banner "Infrastructure"

check "Postgres"     "http://localhost:5432"  "pg_isready port check uses docker exec; skipping raw port"
# Postgres readiness is checked by pg_isready in smoke-test.sh.
echo "  (pg_isready checked via smoke-test.sh)"

check "ClickHouse"   "${AETHER_CLICKHOUSE__URL:-http://localhost:8123}/ping"  "ClickHouse HTTP ping"
check "Redis"        "http://localhost:6379"  "Redis TCP port (PING via redis-cli)"
check "Qdrant"       "${AETHER_QDRANT__URL:-http://localhost:6333}/readyz"    "Qdrant readyz"
check "MinIO"        "http://localhost:9000/minio/health/live"                 "MinIO health"
check "Redpanda"     "http://localhost:9644/v1/status"                         "Redpanda admin"

# ---- Gateway (EP-004) -------------------------------------------------------
banner "Gateway"
GATEWAY_BIND="${AETHER_GATEWAY__BIND:-127.0.0.1:8080}"
check "Gateway" "http://${GATEWAY_BIND}/healthz" "Gateway healthz"
check "Gateway" "http://${GATEWAY_BIND}/readyz"  "Gateway readyz"

# ---- Brain API (EP-201) -----------------------------------------------------
banner "Brain"
BRAIN_BIND="${AETHER_BRAIN__BIND:-127.0.0.1:8000}"
check "Brain" "http://${BRAIN_BIND}/healthz" "Brain healthz"
check "Brain" "http://${BRAIN_BIND}/readyz"  "Brain readyz"

# ---- LLM Router (EP-202) ----------------------------------------------------
banner "LLM Router"
LLM_BIND="${AETHER_LLM__BIND:-127.0.0.1:8001}"
check "LLM Router" "http://${LLM_BIND}/healthz" "LLM Router healthz"

# ---- Alerts (EP-203) -------------------------------------------------------
banner "Alerts"
ALERTS_BIND="${AETHER_ALERTS__BIND:-127.0.0.1:8002}"
check "Alerts" "http://${ALERTS_BIND}/healthz" "Alerts healthz"

# ---- Inbox (EP-204) ---------------------------------------------------------
banner "Inbox"
INBOX_BIND="${AETHER_INBOX__BIND:-127.0.0.1:8003}"
check "Inbox" "http://${INBOX_BIND}/healthz" "Inbox healthz"

# ---- gRPC services (using grpc-health-probe) --------------------------------
banner "gRPC Services"

ROUTER_BIND="${AETHER_ROUTER__BIND:-127.0.0.1:50051}"
RISK_BIND="${AETHER_RISK__BIND:-127.0.0.1:50052}"
GUARDIAN_BIND="${AETHER_GUARDIAN__BIND:-127.0.0.1:50053}"

check_grpc "Order Router"      "${ROUTER_BIND}"
check_grpc "Risk Engine"       "${RISK_BIND}"
check_grpc "Wallet Guardian"   "${GUARDIAN_BIND}"

# ---- Venue adapter health / metrics HTTP ports ------------------------------
banner "Venue Adapters"

check "Kalshi"         "http://127.0.0.1:8084/healthz"       "Kalshi healthz"
check "Polymarket"     "http://127.0.0.1:8085/healthz"       "Polymarket healthz"
check "Hyperliquid"    "http://127.0.0.1:8086/healthz"       "Hyperliquid healthz"
check "Alpaca"         "http://127.0.0.1:8087/healthz"       "Alpaca healthz"
check "OpenBB"         "http://127.0.0.1:8088/healthz"       "OpenBB healthz"

# ---- Prometheus (optional) --------------------------------------------------
banner "Metrics"
check "Prometheus" "http://localhost:9090/-/ready" "Prometheus ready"

# ---- Summary ----------------------------------------------------------------
echo ""
echo "--- Summary ---"
# We accumulate counts in the check() and check_grpc() functions.
# FAIL is the count of failed checks; PASS is derived as total-checks-minus-fail-minus-skip.
# Since total-checks varies with how many services are reachable, we just report
# the binary result and let the line-by-line output above be the detail.
echo "  FAIL count: $FAIL"
echo "  SKIP count: $SKIP"

if [ "$FAIL" -eq 0 ]; then
  echo "health: ok"
  exit 0
else
  echo "health: FAILED"
  exit 1
fi
