"""Pipeline stage 6: embed — chunk text and store embeddings in Qdrant.

Chunks cleaned text into ~500-char segments with 50-char overlap.
Generates content-dependent embeddings (1024-d unit vectors via SHA-256 seed).
Stores chunks in the ``brain_chunks`` Qdrant collection (1024-d, cosine).

Returns the number of chunks stored.

# TODO(EP-202): replace with real embeddings
"""

import logging
import os
import uuid

from server.brain.router_stub import embed_text

logger = logging.getLogger(__name__)

_CHUNK_SIZE = 500
_CHUNK_OVERLAP = 50
_EMBEDDING_DIMENSION = 1024

_AETHER_QDRANT_URL = os.environ.get("AETHER_QDRANT__URL", "http://localhost:6333")
_QDRANT_COLLECTION_CHUNKS = "brain_chunks"


def generate_embedding(text: str) -> list[float]:
    """Generate an embedding through the local EP-202 router contract stub."""
    return embed_text(text, _EMBEDDING_DIMENSION)


def _chunk_text(text: str) -> list[str]:
    """Split cleaned text into overlapping chunks.

    Args:
        text: The cleaned text to split.

    Returns:
        List of text chunks, each ~``_CHUNK_SIZE`` characters with
        ``_CHUNK_OVERLAP`` character overlap.
    """
    if not text:
        return []

    chunks: list[str] = []
    start = 0
    text_len = len(text)

    while start < text_len:
        end = min(start + _CHUNK_SIZE, text_len)
        # Avoid splitting mid-word at the chunk boundary
        if end < text_len:
            # Try to break at a space
            space_pos = text.rfind(" ", start, end)
            if space_pos > start + _CHUNK_SIZE // 2:
                end = space_pos
        chunk = text[start:end].strip()
        if chunk:
            chunks.append(chunk)

        # Advance by chunk_size - overlap (slide the window)
        step = _CHUNK_SIZE - _CHUNK_OVERLAP
        if step <= 0:
            step = 1  # safety: prevent infinite loop
        start += step

        # Guard against pathological overlap where we don't make progress
        if start >= text_len:
            break

        # Final chunk: if we're at the end, take whatever remains
        if start < text_len and text_len - start <= _CHUNK_OVERLAP:
            remaining = text[start:].strip()
            if remaining and (not chunks or remaining != chunks[-1]):
                chunks.append(remaining)
            break

    return chunks


def _generate_stub_embedding(dim: int) -> list[float]:
    """Generate a stub embedding vector.

    Returns a random unit vector for testing (seeded deterministically
    so the same chunk always produces the same vector within a session).

    STUB for EP-201. Real embeddings deferred to EP-202.

    # TODO(EP-202): replace with real embedding model
    """
    return embed_text("aether deterministic router stub", dim)


def _ensure_qdrant_collection() -> object | None:
    """Ensure the ``brain_chunks`` collection exists in Qdrant.

    Returns a ``qdrant_client.QdrantClient`` instance or ``None`` if
    the ``qdrant_client`` package is not installed (graceful degradation).

    Raises:
        Exception: If ``qdrant_client`` is installed but Qdrant is
            unreachable (fail-closed) — the runner catches this and
            parks the object.
    """
    try:
        from qdrant_client import QdrantClient  # noqa: PLC0415
        from qdrant_client.models import Distance, VectorParams  # noqa: PLC0415
    except ImportError:
        logger.warning("embed: qdrant-client not installed — chunks not stored")
        return None

    # Fail-closed: if the client is installed but Qdrant is unreachable,
    # the exception propagates so the runner can park the object.
    client = QdrantClient(url=_AETHER_QDRANT_URL)
    collections = client.get_collections().collections
    existing = {c.name for c in collections}

    if _QDRANT_COLLECTION_CHUNKS not in existing:
        client.create_collection(
            collection_name=_QDRANT_COLLECTION_CHUNKS,
            vectors_config=VectorParams(
                size=_EMBEDDING_DIMENSION,
                distance=Distance.COSINE,
            ),
        )
        logger.debug(
            "embed: created Qdrant collection '%s' (%d-d, cosine)",
            _QDRANT_COLLECTION_CHUNKS,
            _EMBEDDING_DIMENSION,
        )
    return client


async def run(cleaned_text: str, object_id: str, source: str) -> int:
    """Chunk cleaned text, generate stub embeddings, store in Qdrant.

    Args:
        cleaned_text: Text from the clean stage.
        object_id: ULID of the associated BrainObject (used as payload ref).
        source: Source identifier (used as payload metadata).

    Returns:
        Number of chunks stored in Qdrant. Returns 0 if text is empty.
    """
    chunks = _chunk_text(cleaned_text)

    if not chunks:
        logger.debug("embed: no chunks to embed (empty text)")
        return 0

    client = _ensure_qdrant_collection()
    if client is None:
        logger.info(
            "embed: qdrant-client not installed — %d chunks not stored", len(chunks)
        )
        return 0

    points = []
    for i, chunk_text in enumerate(chunks):
        embedding = generate_embedding(chunk_text)
        point_id = str(uuid.uuid5(uuid.NAMESPACE_DNS, f"{object_id}/chunk/{i}"))
        points.append(
            {
                "id": point_id,
                "vector": embedding,
                "payload": {
                    "object_id": object_id,
                    "source": source,
                    "chunk_index": i,
                    "text": chunk_text,
                },
            }
        )

    from qdrant_client.models import Batch  # noqa: PLC0415

    client.upsert(
        collection_name=_QDRANT_COLLECTION_CHUNKS,
        points=Batch(
            ids=[p["id"] for p in points],
            vectors=[p["vector"] for p in points],
            payloads=[p["payload"] for p in points],
        ),
    )
    logger.debug(
        "embed: stored %d chunks in Qdrant '%s'",
        len(points),
        _QDRANT_COLLECTION_CHUNKS,
    )

    return len(chunks)
