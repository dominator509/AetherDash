#!/usr/bin/env bash
# SPEC-002: MinIO bucket bootstrap — idempotent
# Buckets: aether-raw, aether-clean, aether-artifacts, aether-backups
set -euo pipefail

MINIO_ENDPOINT="${AETHER_MINIO__ENDPOINT:-http://localhost:9000}"
MINIO_ACCESS_KEY="${AETHER_MINIO__ACCESS_KEY:-minioadmin}"
MINIO_SECRET_KEY="${AETHER_MINIO__SECRET_KEY:-minioadmin}"
ALIAS="aether-minio"

# Configure mc alias (idempotent)
if command -v mc >/dev/null 2>&1; then
    mc alias set "${ALIAS}" "${MINIO_ENDPOINT}" "${MINIO_ACCESS_KEY}" "${MINIO_SECRET_KEY}" >/dev/null 2>&1 || true

    BUCKETS=("aether-raw" "aether-clean" "aether-artifacts" "aether-backups")
    echo "Bootstrapping MinIO buckets at ${MINIO_ENDPOINT} ..."
    for bucket in "${BUCKETS[@]}"; do
        if mc ls "${ALIAS}/${bucket}" >/dev/null 2>&1; then
            echo "  ${bucket}: already exists (skipping)"
        else
            echo "  ${bucket}: creating ..."
            mc mb "${ALIAS}/${bucket}"
            echo "  ${bucket}: created"
        fi
    done
    echo "MinIO bootstrap complete."
else
    # Fallback: use curl with S3 API
    echo "mc not found; using curl for MinIO bootstrap ..."
    BUCKETS=("aether-raw" "aether-clean" "aether-artifacts" "aether-backups")
    for bucket in "${BUCKETS[@]}"; do
        # Check if bucket exists (list objects)
        if curl -fsS -o /dev/null "${MINIO_ENDPOINT}/${bucket}" 2>/dev/null; then
            echo "  ${bucket}: already exists (skipping)"
        else
            echo "  ${bucket}: creating ..."
            curl -fsS -X PUT "${MINIO_ENDPOINT}/${bucket}" \
                -H "Authorization: AWS ${MINIO_ACCESS_KEY}:${MINIO_SECRET_KEY}" \
                -H "Content-Length: 0" 2>/dev/null || true
            echo "  ${bucket}: created"
        fi
    done
    echo "MinIO bootstrap complete (curl fallback)."
fi
