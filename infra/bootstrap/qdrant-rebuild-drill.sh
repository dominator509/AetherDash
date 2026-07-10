#!/usr/bin/env bash
# SPEC-002 / EP-003 M7: Qdrant reconstruction drill
# Drop all collections, re-bootstrap, assert empty-but-correct schema
set -euo pipefail

QDRANT_URL="${AETHER_QDRANT__URL:-http://localhost:6333}"
PYTHON="${AETHER_PYTHON:-python3}"
command -v "$PYTHON" >/dev/null 2>&1 || PYTHON=python
command -v "$PYTHON" >/dev/null 2>&1 || { echo "ERROR: python not found"; exit 2; }

echo "=== Qdrant Reconstruction Drill ==="

# Collect existing collections
COLLECTIONS=$(curl -fsS "${QDRANT_URL}/collections" | ${PYTHON} -c "
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

# Verify collections and their configuration (fetch each individually for config)
echo "Verifying collections and configuration ..."
for name in brain_chunks market_texts; do
    curl -fsS "${QDRANT_URL}/collections/${name}" | ${PYTHON} -c "
import sys, json
c = json.load(sys.stdin)['result']
cfg = c['config']['params']['vectors']
dims = cfg.get('size')
dist = cfg.get('distance')
assert dims is not None, f'FAIL: ${name} has no vector size'
assert dist == 'Cosine', f'FAIL: ${name} distance={dist}, expected Cosine'
assert c['status'] == 'green', f'FAIL: ${name} status={c[\"status\"]}'
print(f'  ${name}: dims={dims}, distance={dist}, status={c[\"status\"]} OK')
" || { echo "Qdrant reconstruction drill: FAILED for ${name}"; exit 1; }
done
echo "Qdrant reconstruction drill: PASS"
