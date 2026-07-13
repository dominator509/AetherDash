#!/usr/bin/env bash
# Layer: 6 - Verification & Operations
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"

need() { command -v "$1" >/dev/null 2>&1 || { echo "MISSING TOOL: $1"; exit 2; }; }
minver() { # minver <have> <want> -> ok if have >= want
  awk -v have="$1" -v want="$2" 'BEGIN {
    n = split(have, h, "."); m = split(want, w, ".");
    for (i = 1; i <= (n > m ? n : m); i++) {
      hv = (i <= n ? h[i] + 0 : 0); wv = (i <= m ? w[i] + 0 : 0);
      if (hv > wv) exit 0; if (hv < wv) exit 1;
    }
    exit 0
  }'
}

need git
need rustc; need cargo
RV="$(rustc --version | awk '{print $2}')"
minver "$RV" "1.78.0" || { echo "FAIL: rustc $RV < 1.78 (A-03)"; exit 1; }
need node
NV="$(node --version | sed 's/^v//')"
minver "$NV" "20.0.0" || { echo "FAIL: node $NV < 20 (A-04)"; exit 1; }
need pnpm
PV="$(pnpm --version)"
minver "$PV" "9.0.0" || { echo "FAIL: pnpm $PV < 9 (A-04)"; exit 1; }
PYTHON=""
if command -v python3 >/dev/null 2>&1; then PYTHON=python3
elif command -v python >/dev/null 2>&1; then PYTHON=python
else echo "MISSING TOOL: python3/python"; exit 2; fi
"$PYTHON" -c 'import sys; sys.exit(0 if sys.version_info >= (3,11) else 1)' \
  || { echo "FAIL: python < 3.11 (A-05)"; exit 1; }
need uv
need docker
docker compose version >/dev/null 2>&1 || { echo "MISSING TOOL: docker compose plugin (A-06)"; exit 2; }

for t in cargo-nextest cargo-audit buf gitleaks jq curl; do
  if command -v "$t" >/dev/null 2>&1; then echo "optional: $t ok"; else echo "optional: $t missing"; fi
done

echo "preflight: ok"
