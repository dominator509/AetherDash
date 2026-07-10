#!/usr/bin/env bash
# SPEC-002: MinIO bucket bootstrap — idempotent
# Requires: mc (MinIO client). Install: curl -O https://dl.min.io/client/mc/release/
#   or via Docker: docker run --rm --network dev_default --entrypoint mc minio/mc
set -euo pipefail

MINIO_ENDPOINT="${AETHER_MINIO__ENDPOINT:-http://localhost:9000}"
MINIO_ACCESS_KEY="${AETHER_MINIO__ACCESS_KEY:-minioadmin}"
MINIO_SECRET_KEY="${AETHER_MINIO__SECRET_KEY:-minioadmin}"
ALIAS="aether-minio"
BUCKETS=("aether-raw" "aether-clean" "aether-artifacts" "aether-backups")

# Require mc — fail clearly if not installed.
if ! command -v mc >/dev/null 2>&1; then
    echo "ERROR: 'mc' (MinIO Client) not found."
    echo "Install: curl -O https://dl.min.io/client/mc/release/windows-amd64/mc.exe"
    echo "    or: docker run --rm --network dev_default --entrypoint mc minio/mc \\"
    echo "        alias set aether-minio http://minio:9000 minioadmin minioadmin"
    exit 2
fi

echo "Bootstrapping MinIO buckets at ${MINIO_ENDPOINT} ..."
mc alias set "${ALIAS}" "${MINIO_ENDPOINT}" "${MINIO_ACCESS_KEY}" "${MINIO_SECRET_KEY}"

for bucket in "${BUCKETS[@]}"; do
    if mc ls "${ALIAS}/${bucket}" >/dev/null 2>&1; then
        echo "  ${bucket}: already exists (skipping)"
    else
        echo "  ${bucket}: creating ..."
        mc mb "${ALIAS}/${bucket}"
        echo "  ${bucket}: created"
    fi
    # Verify bucket exists post-operation
    if ! mc ls "${ALIAS}/${bucket}" >/dev/null 2>&1; then
        echo "  ${bucket}: VERIFICATION FAILED — bucket not found after creation"
        exit 1
    fi
    echo "  ${bucket}: verified"
done

echo "MinIO bootstrap complete — all ${#BUCKETS[@]} buckets verified."
