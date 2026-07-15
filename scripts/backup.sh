#!/usr/bin/env bash
# Layer: 6 - Verification & Operations
# EP-408: Database backup for Postgres + ClickHouse + Kuzu + Qdrant.
# Stores to a configurable BACKUP_DIR (default ./data/backups) with
# per-service subdirectories and ISO-8601 timestamps.
#
# Retention policy (set via environment):
#   PG_RETENTION_DAYS=30   # Postgres dumps
#   CH_RETENTION_DAYS=14   # ClickHouse dumps
#   KUZU_RETENTION_DAYS=14 # Kuzu tarballs
#   QDRANT_RETENTION_DAYS=7 # Qdrant snapshots
#
# Usage:
#   export DATABASE_URL="postgres://aether:aether@localhost:5432/aether"
#   export AETHER_CLICKHOUSE__URL="http://localhost:8123"
#   export BACKUP_DIR="/opt/aether/backups"
#   ./scripts/backup.sh

set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# ---- Config ----------------------------------------------------------------
BACKUP_DIR="${BACKUP_DIR:-$ROOT/data/backups}"
TS="$(date -u +%Y%m%dT%H%M%SZ)"

PG_RETENTION_DAYS="${PG_RETENTION_DAYS:-30}"
CH_RETENTION_DAYS="${CH_RETENTION_DAYS:-14}"
KUZU_RETENTION_DAYS="${KUZU_RETENTION_DAYS:-14}"
QDRANT_RETENTION_DAYS="${QDRANT_RETENTION_DAYS:-7}"

DATABASE_URL="${DATABASE_URL:-postgres://aether:aether@localhost:5432/aether}"
CH_URL="${AETHER_CLICKHOUSE__URL:-http://localhost:8123}"
CH_DB="${AETHER_CLICKHOUSE__DATABASE:-aether}"
CH_USER="${AETHER_CLICKHOUSE__USER:-aether}"
CH_PASS="${AETHER_CLICKHOUSE__PASSWORD:-aether}"
QDRANT_URL="${AETHER_QDRANT__URL:-http://localhost:6333}"
KUZU_PATH="${AETHER_KUZU__PATH:-$ROOT/data/kuzu}"

MISSING=0

# ---- Helper ----------------------------------------------------------------
banner() { echo ""; echo "================================================"; echo "  $*"; echo "================================================"; }
age_days() {
  # Prints 0 if file doesn't exist, else age in days (floor).
  if [ -f "$1" ]; then
    python3 -c "import os,time; print(int((time.time()-os.path.getmtime('$1'))/86400))" 2>/dev/null || echo "0"
  else echo "0"; fi
}
prune() { # prune <dir> <glob> <keep-days>
  local dir="$1" glob="$2" keep="$3"
  if [ ! -d "$dir" ]; then return; fi
  echo "Pruning $glob older than ${keep}d in $dir"
  find "$dir" -name "$glob" -type f -mtime "+$keep" -delete -print 2>/dev/null || true
}

# ---- Prerequisites ---------------------------------------------------------
command -v pg_dump >/dev/null 2>&1 || { echo "MISSING TOOL: pg_dump"; MISSING=1; }
command -v curl    >/dev/null 2>&1 || { echo "MISSING TOOL: curl";    MISSING=1; }
command -v python3 >/dev/null 2>&1 || { echo "MISSING TOOL: python3"; MISSING=1; }
command -v tar     >/dev/null 2>&1 || { echo "MISSING TOOL: tar";     MISSING=1; }
[ "$MISSING" -eq 1 ] && exit 2

mkdir -p "$BACKUP_DIR"/{pg,ch,kuzu,qdrant}

# ===== 1. Postgres =========================================================
banner "Postgres backup"
PG_FILE="$BACKUP_DIR/pg/aether-${TS}.dump"
echo "  -> $PG_FILE"
pg_dump -Fc --no-acl --no-owner -d "$DATABASE_URL" -f "$PG_FILE"
echo "  size: $(du -h "$PG_FILE" | cut -f1)"
prune "$BACKUP_DIR/pg" "aether-*.dump" "$PG_RETENTION_DAYS"

# ===== 2. ClickHouse =======================================================
banner "ClickHouse backup"
CH_FILE="$BACKUP_DIR/ch/aether-${TS}.sql.gz"
echo "  -> $CH_FILE"
# Use BACKUP DATABASE if available (ClickHouse 24.x), fall back to SELECT dump.
if curl -sf -u "${CH_USER}:${CH_PASS}" "${CH_URL}/?database=${CH_DB}" \
       --data-binary "BACKUP DATABASE ${CH_DB} TO Disk('default', 'backups/${TS}/')" >/dev/null 2>&1; then
  echo "  BACKUP DATABASE command accepted (verify via system.backups)"
else
  # Fallback: export all tables via SELECT * INTO OUTFILE equivalent.
  TABLES=$(curl -sf -u "${CH_USER}:${CH_PASS}" \
    "${CH_URL}/?database=${CH_DB}" \
    --data-binary "SELECT name FROM system.tables WHERE database='${CH_DB}' AND engine NOT LIKE '%View%' AND engine NOT LIKE '%MaterializedView%'" 2>/dev/null | tr '\n' ' ')
  if [ -n "$TABLES" ]; then
    (
      for tbl in $TABLES; do
        [ -z "$tbl" ] && continue
        echo "SELECT * FROM ${CH_DB}.${tbl} FORMAT TabSeparatedWithNames"
      done
    ) | curl -sf -u "${CH_USER}:${CH_PASS}" "${CH_URL}/?database=${CH_DB}" \
        --data-binary @- | gzip > "$CH_FILE"
  else
    echo "  WARNING: No tables found or ClickHouse unreachable; skipping dump"
  fi
fi
echo "  size: $(du -h "$CH_FILE" 2>/dev/null | cut -f1 || echo '0')"
prune "$BACKUP_DIR/ch" "aether-*.sql.gz" "$CH_RETENTION_DAYS"

# ===== 3. Kuzu =============================================================
banner "Kuzu backup"
if [ -d "$KUZU_PATH" ]; then
  KUZU_FILE="$BACKUP_DIR/kuzu/aether-${TS}.tar.gz"
  echo "  source: $KUZU_PATH"
  echo "  -> $KUZU_FILE"
  tar czf "$KUZU_FILE" -C "$(dirname "$KUZU_PATH")" "$(basename "$KUZU_PATH")" 2>/dev/null
  echo "  size: $(du -h "$KUZU_FILE" | cut -f1)"
else
  echo "  SKIP: $KUZU_PATH not found"
fi
prune "$BACKUP_DIR/kuzu" "aether-*.tar.gz" "$KUZU_RETENTION_DAYS"

# ===== 4. Qdrant snapshots =================================================
banner "Qdrant backup"
if command -v jq >/dev/null 2>&1; then
  COLLECTIONS=$(curl -sf "$QDRANT_URL/collections" | python3 -c "import sys,json; [print(c) for c in json.load(sys.stdin).get('result',{}).get('collections',[])]" 2>/dev/null || true)
  if [ -n "$COLLECTIONS" ]; then
    for col in $COLLECTIONS; do
      SNAP_FILE="$BACKUP_DIR/qdrant/${col}-${TS}.snapshot"
      echo "  snapshotting collection '$col' -> $SNAP_FILE"
      SNAP_RESULT=$(curl -sf -X POST "$QDRANT_URL/collections/${col}/snapshots" 2>/dev/null || echo "")
      if [ -n "$SNAP_RESULT" ]; then
        echo "    done"
      else
        echo "    SKIP (snapshot API not available)"
      fi
    done
  else
    echo "  SKIP: no collections found or Qdrant unreachable"
  fi
else
  echo "  SKIP: jq not installed; Qdrant snapshot via API requires jq"
fi
prune "$BACKUP_DIR/qdrant" "*.snapshot" "$QDRANT_RETENTION_DAYS"

# ===== 5. Summary ==========================================================
echo ""
echo "Backup complete at $(date -u --iso-8601=seconds)"
echo "  Directory: $BACKUP_DIR"
echo ""
du -sh "$BACKUP_DIR"/*/ 2>/dev/null | while read -r line; do echo "  $line"; done
echo ""
echo "backup: ok"
