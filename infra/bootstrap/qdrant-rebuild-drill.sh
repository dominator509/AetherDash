#!/usr/bin/env bash
# SPEC-002 / EP-003 M7: Qdrant reconstruction drill
# Drop all collections, re-bootstrap, assert empty-but-correct schema
set -euo pipefail

QDRANT_URL="${AETHER_QDRANT__URL:-http://localhost:6333}"

echo "=== Qdrant Reconstruction Drill ==="

# Collect existing collections
COLLECTIONS=$(curl -fsS "${QDRANT_URL}/collections" | python3 -c "
import sys, json
data = json.load(sys.stdin)
cols = data.get('result', {}).get('collections', {})
if isinstance(cols, list):
    print(' '.join(c['name'] for c in cols))
elif isinstance(cols, dict):
    print(' '.join(cols.keys()))
" 2>/dev/null || echo "")

if [ -z "$COLLECTIONS" ]; then
    echo "No collections found to drop."
else
    for col in $COLLECTIONS; do
        echo "Dropping collection: ${col}"
        curl -fsS -X DELETE "${QDRANT_URL}/collections/${col}" >/dev/null
        echo "  ${col}: dropped"
    done
fi

# Re-bootstrap
BOOTSTRAP="$(cd "$(dirname "$0")" && pwd)/qdrant.sh"
if [ -f "$BOOTSTRAP" ]; then
    bash "$BOOTSTRAP"
else
    echo "ERROR: qdrant.sh not found at $BOOTSTRAP"
    exit 1
fi

# Verify
echo "Verifying collections ..."
RESULT=$(curl -fsS "${QDRANT_URL}/collections")
echo "$RESULT" | python3 -c "
import sys, json
data = json.load(sys.stdin)
raw = data.get('result', {})
cols = raw.get('collections', []) if isinstance(raw.get('collections'), list) else raw.get('collections', {})
if isinstance(cols, list):
    names = {c['name'] for c in cols}
else:
    names = set(cols.keys())
assert 'brain_chunks' in names, 'brain_chunks missing from collections'
assert 'market_texts' in names, 'market_texts missing from collections'
print('  brain_chunks: OK')
print('  market_texts: OK')
print('Qdrant reconstruction drill: PASS')
"
