#!/usr/bin/env bash
# Layer: 6 - Verification & Operations
# EP-408: Database restore for Postgres + ClickHouse + Kuzu + Qdrant.
#
# IMPORTANT: Restore is a HUMAN-supervised operation (S6).  This script
# never runs unattended.  By default it prints what it WOULD do (dry-run);
# pass --confirm to actually execute.
#
# Usage:
#   ./scripts/restore.sh                           # dry-run, list snapshots
#   ./scripts/restore.sh --confirm pg <file>        # restore Postgres
#   ./scripts/restore.sh --confirm ch <file>        # restore ClickHouse
#   ./scripts/restore.sh --confirm kuzu <file>      # restore Kuzu
#   ./scripts/restore.sh --confirm qdrant <col> <snapshot-id>  # Qdrant snapshot
#   ./scripts/restore.sh --list                     # list available backups

set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BACKUP_DIR="${BACKUP_DIR:-$ROOT/data/backups}"
DATABASE_URL="${DATABASE_URL:-postgres://aether:aether@localhost:5432/aether}"
CH_URL="${AETHER_CLICKHOUSE__URL:-http://localhost:8123}"
CH_DB="${AETHER_CLICKHOUSE__DATABASE:-aether}"
CH_USER="${AETHER_CLICKHOUSE__USER:-aether}"
CH_PASS="${AETHER_CLICKHOUSE__PASSWORD:-aether}"
QDRANT_URL="${AETHER_QDRANT__URL:-http://localhost:6333}"
KUZU_PATH="${AETHER_KUZU__PATH:-$ROOT/data/kuzu}"

MISSING=0

command -v pg_restore >/dev/null 2>&1 || { echo "MISSING TOOL: pg_restore"; MISSING=1; }
command -v curl        >/dev/null 2>&1 || { echo "MISSING TOOL: curl";        MISSING=1; }
command -v python3     >/dev/null 2>&1 || { echo "MISSING TOOL: python3";     MISSING=1; }
[ "$MISSING" -eq 1 ] && exit 2

usage() {
  echo "Usage:"
  echo "  $0                                      # dry-run (list snapshots, show what would happen)"
  echo "  $0 --list                               # list available backups"
  echo "  $0 --confirm pg  <file>.dump            # restore Postgres"
  echo "  $0 --confirm ch  <file>.sql.gz          # restore ClickHouse"
  echo "  $0 --confirm kuzu <file>.tar.gz         # restore Kuzu"
  echo "  $0 --confirm qdrant <collection> <snapshot-id>  # restore Qdrant snapshot"
  echo ""
  echo "Backup directory: $BACKUP_DIR"
  echo "Postgres target:  ${DATABASE_URL}"
  echo "ClickHouse URL:   ${CH_URL}"
  echo "Kuzu path:        ${KUZU_PATH}"
  echo "Qdrant URL:       ${QDRANT_URL}"
}

# ---- List available backups -------------------------------------------------
list_backups() {
  echo "=== Available backups ==="
  echo ""
  echo "--- Postgres ($BACKUP_DIR/pg/) ---"
  ls -1ht "$BACKUP_DIR"/pg/ 2>/dev/null || echo "  (none)"
  echo ""
  echo "--- ClickHouse ($BACKUP_DIR/ch/) ---"
  ls -1ht "$BACKUP_DIR"/ch/ 2>/dev/null || echo "  (none)"
  echo ""
  echo "--- Kuzu ($BACKUP_DIR/kuzu/) ---"
  ls -1ht "$BACKUP_DIR"/kuzu/ 2>/dev/null || echo "  (none)"
  echo ""
  echo "--- Qdrant snapshots ($BACKUP_DIR/qdrant/) ---"
  ls -1ht "$BACKUP_DIR"/qdrant/ 2>/dev/null || echo "  (none)"
}

# ---- Restore Postgres -------------------------------------------------------
restore_pg() {
  local file="$1"
  if [ ! -f "$file" ]; then echo "ERROR: file not found: $file"; exit 1; fi
  echo "WARNING: This will DROP and recreate the target database."
  echo "  Target: $DATABASE_URL"
  echo "  Source: $file"
  if [ "$DRY_RUN" = true ]; then
    echo "  DRY-RUN: would execute:"
    echo "    dropdb --force ..."
    echo "    createdb ..."
    echo "    pg_restore -d ... $file"
    return
  fi
  # Extract database name from DATABASE_URL
  DB_NAME="$(python3 -c "from urllib.parse import urlparse; print(urlparse('${DATABASE_URL}').path.lstrip('/'))" 2>/dev/null || echo "aether")"
  echo "  Dropping database '${DB_NAME}' ..."
  dropdb --force --if-exists "$DB_NAME" 2>/dev/null || true
  echo "  Creating database '${DB_NAME}' ..."
  createdb "$DB_NAME" 2>/dev/null || true
  echo "  Restoring from $file ..."
  pg_restore --clean --if-exists --no-acl --no-owner -d "$DATABASE_URL" "$file"
  echo "Postgres restore complete."
}

# ---- Restore ClickHouse -----------------------------------------------------
restore_ch() {
  local file="$1"
  if [ ! -f "$file" ]; then echo "ERROR: file not found: $file"; exit 1; fi
  echo "WARNING: This will REPLACE the ClickHouse database ${CH_DB}."
  echo "  URL:  ${CH_URL}"
  echo "  File: $file"
  if [ "$DRY_RUN" = true ]; then
    echo "  DRY-RUN: would drop database, recreate, then replay file via clickhouse-client"
    return
  fi
  echo "  Dropping database ${CH_DB} ..."
  curl -sf -u "${CH_USER}:${CH_PASS}" "${CH_URL}/" \
    --data-binary "DROP DATABASE IF EXISTS ${CH_DB}" >/dev/null
  echo "  Recreating database ${CH_DB} ..."
  curl -sf -u "${CH_USER}:${CH_PASS}" "${CH_URL}/" \
    --data-binary "CREATE DATABASE IF NOT EXISTS ${CH_DB}" >/dev/null
  echo "  Replaying backup ..."
  if [[ "$file" == *.gz ]]; then
    gunzip -c "$file" | curl -sf -u "${CH_USER}:${CH_PASS}" \
      "${CH_URL}/?database=${CH_DB}" --data-binary @- >/dev/null
  else
    curl -sf -u "${CH_USER}:${CH_PASS}" \
      "${CH_URL}/?database=${CH_DB}" --data-binary @"$file" >/dev/null
  fi
  echo "ClickHouse restore complete."
}

# ---- Restore Kuzu -----------------------------------------------------------
restore_kuzu() {
  local file="$1"
  if [ ! -f "$file" ]; then echo "ERROR: file not found: $file"; exit 1; fi
  echo "WARNING: This will REPLACE the Kuzu graph at ${KUZU_PATH}."
  echo "  File: $file"
  if [ "$DRY_RUN" = true ]; then
    echo "  DRY-RUN: would remove ${KUZU_PATH}, then extract tarball"
    return
  fi
  echo "  Removing existing Kuzu data ..."
  rm -rf "$KUZU_PATH"
  echo "  Extracting $file ..."
  mkdir -p "$(dirname "$KUZU_PATH")"
  tar xzf "$file" -C "$(dirname "$KUZU_PATH")"
  echo "Kuzu restore complete."
}

# ---- Restore Qdrant ---------------------------------------------------------
restore_qdrant() {
  local collection="$1" snapshot_id="$2"
  if [ -z "$collection" ] || [ -z "$snapshot_id" ]; then
    echo "ERROR: qdrant restore requires <collection> <snapshot-id>"
    echo "  List snapshots: curl ${QDRANT_URL}/collections/<collection>/snapshots"
    exit 1
  fi
  echo "WARNING: This will REPLACE the Qdrant collection '${collection}'."
  echo "  Snapshot ID: ${snapshot_id}"
  if [ "$DRY_RUN" = true ]; then
    echo "  DRY-RUN: would POST to ${QDRANT_URL}/collections/${collection}/snapshots/${snapshot_id}/recover"
    return
  fi
  echo "  Recovering from snapshot ..."
  curl -sf -X PUT "${QDRANT_URL}/collections/${collection}/snapshots/${snapshot_id}/recover" \
    -H "Content-Type: application/json" \
    -d "{}" >/dev/null || echo "  WARNING: snapshot recovery may have failed (check Qdrant logs)"
  echo "Qdrant restore complete."
}

# ---- Main -------------------------------------------------------------------
DRY_RUN=true
if [ $# -eq 0 ] || { [ "$1" = "--list" ] && [ $# -eq 1 ]; }; then
  list_backups
  echo ""
  echo "Run '$0 --confirm <service> <file>' to restore (HUMAN only)."
  exit 0
fi

if [ "$1" = "--confirm" ]; then
  DRY_RUN=false
  shift
  CMD="${1:-}"
  shift || true
fi

case "${CMD:-}" in
  pg)
    restore_pg "${1:-}"
    ;;
  ch)
    restore_ch "${1:-}"
    ;;
  kuzu)
    restore_kuzu "${1:-}"
    ;;
  qdrant)
    restore_qdrant "${1:-}" "${2:-}"
    ;;
  *)
    usage
    exit 1
    ;;
esac

echo "restore: ok"
