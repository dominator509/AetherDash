#!/usr/bin/env bash
# SPEC-002: Qdrant collection bootstrap — idempotent
set -euo pipefail

QDRANT_URL="${AETHER_QDRANT__URL:-http://localhost:6333}"
DIMS="${AETHER_EMBED__DIMS:-1024}"
PYTHON="${AETHER_PYTHON:-python3}"
# Fallback python3 -> python
command -v "$PYTHON" >/dev/null 2>&1 || PYTHON=python
command -v "$PYTHON" >/dev/null 2>&1 || { echo "ERROR: python not found"; exit 2; }

echo "Bootstrapping Qdrant collections at ${QDRANT_URL} (dims=${DIMS}) ..."

create_collection() {
    local name="$1"
    local http_code
    http_code=$(curl -sS -o /dev/null -w "%{http_code}" "${QDRANT_URL}/collections/${name}" 2>&1)

    if [ "$http_code" = "200" ]; then
        echo "  ${name}: already exists (skipping)"
        # Verify existing config
        local config
        config=$(curl -fsS "${QDRANT_URL}/collections/${name}")
        echo "$config" | ${PYTHON} -c "
import sys, json
c = json.load(sys.stdin)['result']['config']['params']['vectors']
assert c['size'] == ${DIMS}, f'vector size mismatch: {c[\"size\"]} != ${DIMS}'
assert c['distance'] == 'Cosine', f'distance mismatch: {c[\"distance\"]}'
" || { echo "  ${name}: config verification FAILED"; exit 1; }
        echo "  ${name}: config verified (dims=${DIMS}, distance=Cosine)"
    elif [ "$http_code" = "404" ]; then
        echo "  ${name}: creating ..."
        local create_resp
        create_resp=$(curl -fsS -X PUT "${QDRANT_URL}/collections/${name}" \
            -H 'Content-Type: application/json' \
            -d "{\"vectors\": {\"size\": ${DIMS}, \"distance\": \"Cosine\"}}")
        if echo "$create_resp" | grep -q '"status":"ok"'; then
            # Verify created collection configuration
            local verify_resp
            verify_resp=$(curl -fsS "${QDRANT_URL}/collections/${name}")
            echo "$verify_resp" | ${PYTHON} -c "
import sys, json
c = json.load(sys.stdin)['result']['config']['params']['vectors']
assert c['size'] == ${DIMS}, f'vector size mismatch: {c[\"size\"]} != ${DIMS}'
assert c['distance'] == 'Cosine', f'distance mismatch: {c[\"distance\"]}'
            " || { echo "  ${name}: config verification FAILED"; exit 1; }
            echo "  ${name}: created and verified"
        else
            echo "  ${name}: creation FAILED — response: ${create_resp}"
            exit 1
        fi
    else
        echo "  ${name}: unexpected HTTP ${http_code} from Qdrant — is Qdrant running?"
        exit 1
    fi
}

create_collection "brain_chunks"
create_collection "market_texts"
echo "Qdrant bootstrap complete."
