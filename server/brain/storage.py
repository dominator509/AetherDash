"""MinIO raw-content storage for Brain objects.

Handles the raw content lake (``aether-raw`` bucket).
Content is stored write-once by SHA-256 content hash at key:
``raw/{source}/{yyyy}/{mm}/{dd}/{sha256}``
"""

import hashlib
import io
import os
from datetime import UTC, datetime

from minio import Minio
from minio.error import S3Error

_AETHER_MINIO_ENDPOINT = os.environ.get("AETHER_MINIO__ENDPOINT", "localhost:9000")
_AETHER_MINIO_ACCESS_KEY = os.environ.get("AETHER_MINIO__ACCESS_KEY", "minioadmin")
_AETHER_MINIO_SECRET_KEY = os.environ.get("AETHER_MINIO__SECRET_KEY", "minioadmin")

_BUCKET_RAW = "aether-raw"
_BUCKET_CLEAN = "aether-clean"

_client: Minio | None = None


def _get_client() -> Minio:
    global _client  # noqa: PLW0603
    if _client is None:
        endpoint = _AETHER_MINIO_ENDPOINT
        for prefix in ("https://", "http://"):
            if endpoint.startswith(prefix):
                endpoint = endpoint[len(prefix) :]
                break
        _client = Minio(
            endpoint=endpoint,
            access_key=_AETHER_MINIO_ACCESS_KEY,
            secret_key=_AETHER_MINIO_SECRET_KEY,
            secure=_AETHER_MINIO_ENDPOINT.startswith("https://"),
        )
    return _client


def _ensure_bucket(client: Minio, bucket_name: str) -> None:
    """Create a bucket if it does not exist."""
    if not client.bucket_exists(bucket_name):
        client.make_bucket(bucket_name)


def _sanitize_source(source: str) -> str:
    """Sanitize a source string for use in MinIO object keys.

    MinIO/S3 object keys must not contain ``://``, and some characters are
    restricted or cause problems.  We replace them with safe equivalents.
    """
    s = source.replace("://", "/")
    s = s.replace(":", "-")
    s = s.replace("@", "-")
    s = s.replace("?", "_")
    s = s.replace(" ", "_")
    return s


def store_raw(content_bytes: bytes, source: str) -> tuple[str, str]:
    """Store raw content bytes in MinIO.

    Returns:
        Tuple of ``(sha256_hex, minio_key)``.
    """
    sha256_hash = hashlib.sha256(content_bytes).hexdigest()
    now = datetime.now(UTC)
    safe_source = _sanitize_source(source)
    key = f"raw/{safe_source}/{now:%Y/%m/%d}/{sha256_hash}"
    client = _get_client()
    _ensure_bucket(client, _BUCKET_RAW)
    client.put_object(
        _BUCKET_RAW,
        key,
        io.BytesIO(content_bytes),
        length=len(content_bytes),
    )
    return sha256_hash, key


def get_raw(key: str) -> bytes:
    """Retrieve raw content bytes from MinIO by key."""
    client = _get_client()
    response = client.get_object(_BUCKET_RAW, key)
    try:
        return response.read()
    finally:
        response.close()
        response.release_conn()


def store_clean(content_bytes: bytes, source: str) -> tuple[str, str]:
    """Store cleaned content bytes in MinIO ``aether-clean`` bucket.

    Returns:
        Tuple of ``(sha256_hex, minio_key)``.
    """
    sha256_hash = hashlib.sha256(content_bytes).hexdigest()
    now = datetime.now(UTC)
    safe_source = _sanitize_source(source)
    key = f"clean/{safe_source}/{now:%Y/%m/%d}/{sha256_hash}"
    client = _get_client()
    _ensure_bucket(client, _BUCKET_CLEAN)
    client.put_object(
        _BUCKET_CLEAN,
        key,
        io.BytesIO(content_bytes),
        length=len(content_bytes),
    )
    return sha256_hash, key


def get_clean(key: str) -> bytes:
    """Retrieve cleaned content bytes from MinIO by key."""
    client = _get_client()
    response = client.get_object(_BUCKET_CLEAN, key)
    try:
        return response.read()
    finally:
        response.close()
        response.release_conn()


def object_exists(sha256_hash: str, source: str) -> bool:
    """Check if content with the given SHA-256 and source exists in MinIO.

    Probes the current-day prefix for the object.
    Authoritative deduplication is done via Postgres (``store.object_exists_by_hash``).
    """
    client = _get_client()
    now = datetime.now(UTC)
    safe_source = _sanitize_source(source)
    prefix = f"raw/{safe_source}/{now:%Y/%m/%d}/{sha256_hash}"
    try:
        client.stat_object(_BUCKET_RAW, prefix)
        return True
    except S3Error:
        return False
