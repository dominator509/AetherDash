#!/usr/bin/env bash
# SPEC-002: Qdrant collection bootstrap — idempotent
# Collections: brain_chunks, market_texts
# Dimensions: AETHER_EMBED__DIMS (default 1024), cosine distance
set -euo pipefail

QDRANT_URL="${AETHER_QDRANT__URL:-http://localhost:6333}"
DIMS="${AETHER_EMBED__DIMS:-1024}"

echo "Bootstrapping Qdrant collections at ${QDRANT_URL} (dims=${DIMS}) ..."

create_collection() {
    local name="$1"
    local exists
    exists=$(curl -fsS "${QDRANT_URL}/collections/${name}" 2>/dev/null || true)
    if echo "$exists" | grep -q '"status":"ok"'; then
        echo "  ${name}: already exists (skipping)"
    else
        echo "  ${name}: creating ..."
        curl -fsS -X PUT "${QDRANT_URL}/collections/${name}" \
            -H 'Content-Type: application/json' \
            -d "{
                \"vectors\": {
                    \"size\": ${DIMS},
                    \"distance\": \"Cosine\"
                }
            }" >/dev/null
        echo "  ${name}: created"
    fi
}

create_collection "brain_chunks"
create_collection "market_texts"

echo "Qdrant bootstrap complete."
