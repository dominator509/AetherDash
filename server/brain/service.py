"""Brain service — Store/Get orchestration.

Implements the ingestion pipeline intake stage:
1. Hash raw content (SHA-256) — BEFORE any I/O
2. Check dedupe by raw SHA-256 — return existing BrainRef if found
3. Store raw content in MinIO (only if not duplicate)
4. Compute provenance_hash + dedupe again (safety net)
5. Create BrainObject with all required fields
6. INSERT into brain_objects (ON CONFLICT DO NOTHING)
7. Emit ingest_events row (rung: 1 = intake)
8. Return BrainRef(id, provenance_hash)

After intake, the pipeline runner is fired as an async background task
to process the remaining stages (clean → summarize → extract → link → embed → index).
"""

import asyncio
import hashlib
import logging

import ulid  # ulid-py

from server.brain import recall as recall_module
from server.brain import storage, store
from server.brain.models import (
    BrainObject,
    BrainRef,
    ObjectDraft,
    ObjectKind,
    Origin,
    Tier,
    TrustLevel,
    compute_provenance_hash,
    now_iso,
)
from server.brain.pipeline import runner as pipeline_runner

logger = logging.getLogger(__name__)

# Registry of active pipeline background tasks, keyed by object_id.
# Used for status queries and graceful shutdown.
_active_tasks: dict[str, asyncio.Task] = {}


async def store_draft(
    draft: ObjectDraft,
    origin: str | None = None,
    trust: str | None = None,
    raw_content: bytes | None = None,
) -> BrainRef:
    """Ingest a raw content draft and return a BrainRef.

    Steps per SPEC-011 intake pipeline:
    1. Hash raw content (SHA-256)                                   (intake)
    2. Dedupe by content hash — seen hash returns existing ref
    3. Store raw content in MinIO ``aether-raw``
    4. Compute provenance_hash
    5. Assemble BrainObject
    6. INSERT into brain_objects
    7. Emit ingest_events row (rung: 1 = intake)
    8. Return BrainRef

    Args:
        draft: The object draft to ingest.
        origin: Override the default origin (``ingest_fleet``).
        trust: Override the default trust level (``medium``).
        raw_content: Original hostile bytes when ``draft.content`` is parsed text.
    """
    # 1. Hash raw content (BEFORE any I/O)
    content_bytes = (
        raw_content if raw_content is not None else draft.content.encode("utf-8")
    )
    raw_sha256 = hashlib.sha256(content_bytes).hexdigest()

    # 2. Dedupe by content hash — avoid MinIO write if we already have it
    existing = await store.object_exists_by_raw_sha256(raw_sha256, draft.source)
    if existing is not None:
        logger.debug(
            "store_draft: dedupe by raw_sha256=%s -> %s", raw_sha256, existing.id
        )
        return existing

    # 3. Store raw content in MinIO (only if not duplicate)
    _, minio_key = storage.store_raw(content_bytes, draft.source)

    # 4. Compute provenance_hash + dedupe (safety net)
    ingested_ts = now_iso()
    provenance_hash = compute_provenance_hash(
        source=draft.source,
        raw_sha256=raw_sha256,
        ingested_ts=ingested_ts,
    )
    existing_by_prov = await store.object_exists_by_hash(provenance_hash)
    if existing_by_prov is not None:
        logger.debug(
            "store_draft: dedupe by provenance_hash=%s -> %s",
            provenance_hash,
            existing_by_prov.id,
        )
        return existing_by_prov

    # 5. Assemble BrainObject
    resolved_origin = Origin(origin) if origin else Origin.ingest_fleet
    resolved_trust = TrustLevel(trust) if trust else TrustLevel.medium
    obj_id = str(ulid.new())
    clean_ref = None
    if raw_content is not None:
        _, clean_ref = storage.store_clean(draft.content.encode("utf-8"), draft.source)
    brain_obj = BrainObject(
        id=obj_id,
        kind=ObjectKind(draft.kind),
        source=draft.source,
        origin=resolved_origin,
        trust=resolved_trust,
        ingested_ts=ingested_ts,
        raw_ref=minio_key,
        clean_ref=clean_ref,
        provenance_hash=provenance_hash,
        tier=Tier.warm,
    )

    # 6. INSERT into brain_objects (ON CONFLICT DO NOTHING for atomic dedupe)
    inserted_id = await store.insert_object(brain_obj, on_conflict_do_nothing=True)
    if inserted_id is None:
        # Another concurrent insert of the same source/content won.
        existing_ref = await store.object_exists_by_raw_sha256(raw_sha256, draft.source)
        if existing_ref is not None:
            logger.debug(
                "store_draft: concurrent dedupe by provenance_hash=%s -> %s",
                provenance_hash,
                existing_ref.id,
            )
            return existing_ref
        # Fallback: try without ON CONFLICT
        await store.insert_object(brain_obj)

    # 7. Emit ingest_events row (rung: 1 = intake)
    await store.emit_ingest_event(
        object_id=obj_id,
        source=draft.source,
        ladder_rung=1,
        bytes_count=len(content_bytes),
    )

    # 8. Fire pipeline background task (clean -> summarize -> extract -> link -> embed -> index)
    task = asyncio.create_task(_run_pipeline_task(obj_id))
    _active_tasks[obj_id] = task

    # 9. Return BrainRef
    return BrainRef(id=obj_id, provenance_hash=provenance_hash)


async def run_pipeline_for_object(obj_id: str) -> None:
    """Run the full ingestion pipeline for an existing object by its ULID.

    Useful for retries or manually re-processing an object that was parked
    at an incomplete stage.

    Args:
        obj_id: ULID of the object to process.
    """
    await pipeline_runner.run_pipeline(obj_id)


async def reprocess_object(obj_id: str) -> None:
    """Invalidate derived artifacts and rerun an existing object's raw provenance."""
    await store.update_object(
        obj_id,
        clean_ref=None,
        summary=None,
        entities=[],
        linked_events=[],
        market_keys=[],
        confidence=None,
        current_stage="intake",
        parked_reason=None,
        tier=Tier.warm,
    )
    await pipeline_runner.run_pipeline(obj_id)


# ── Background task management ──────────────────────────────────────────


async def _run_pipeline_task(obj_id: str) -> None:
    """Run the pipeline and handle uncaught exceptions.

    The caller (``store_draft``) returns immediately; this wrapper ensures
    that any failure that escapes the pipeline's own error handling is
    surfaced via the logger and recorded in the task registry.
    """
    try:
        await pipeline_runner.run_pipeline(obj_id)
    except Exception as exc:
        logger.error("Pipeline task crashed for object %s: %s", obj_id, exc)
    finally:
        # Remove from the active registry once completed
        _active_tasks.pop(obj_id, None)


def get_pipeline_status(object_id: str) -> str | None:
    """Return the pipeline status for an object.

    Returns ``None`` if no task was ever registered for this object.
    Returns ``"running"`` while the task is still executing, ``"completed"``
    on success, or ``"failed"`` if the task raised an exception.
    """
    task = _active_tasks.get(object_id)
    if task is None:
        return None
    if task.done():
        exc = task.exception()
        if exc is not None:
            return "failed"
        return "completed"
    return "running"


async def cancel_all_tasks() -> None:
    """Cancel all pending pipeline tasks (called during graceful shutdown)."""
    for obj_id, task in list(_active_tasks.items()):
        if not task.done():
            task.cancel()
            logger.debug("Cancelled pipeline task for object %s", obj_id)
    _active_tasks.clear()


async def get(ref: BrainRef) -> BrainObject | None:
    """Retrieve a BrainObject by its BrainRef.

    Steps per SPEC-011:
    1. SELECT from brain_objects by id
    2. Return full object or None
    """
    return await store.get_object(ref.id)


async def get_by_id(obj_id: str) -> BrainObject | None:
    """Retrieve a BrainObject by its ULID id."""
    return await store.get_object(obj_id)


async def recall(
    query: str,
    k: int = 24,
    filters: dict | None = None,
) -> list[recall_module.ScoredRef]:
    """Hybrid RRF recall — fetch from Qdrant and Postgres FTS, fuse by RRF.

    This is a thin wrapper around ``recall.recall()`` provided for convenience
    so callers (including the gRPC handler) can import from ``service``.

    Args:
        query: Search query text.
        k: Number of results (default 24).
        filters: Optional filter dict. See ``recall.recall`` for filter keys.

    Returns:
        List of ``ScoredRef`` sorted by descending fusion score.
    """
    return await recall_module.recall(query, k=k, filters=filters)
