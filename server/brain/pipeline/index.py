"""Pipeline stage 7: index — upsert brain_objects row, flip visibility.

Updates the ``brain_objects`` Postgres row with all accumulated pipeline
fields (clean_ref, summary, entities, linked_events, market_keys, etc.).
Sets ``tier`` to ``hot`` for new objects, making them recallable.

The FTS column (``fts_vector``) is a generated column computed from
``summary`` and ``kind``. Updating ``summary`` updates it automatically.
"""

import logging

from server.brain import store
from server.brain.models import Tier

logger = logging.getLogger(__name__)


async def run(
    obj_id: str,
    source: str,
    clean_ref: str | None,
    summary: str | None,
    entities: list[str],
    linked_events: list[str],
    market_keys: list[str],
    confidence: float | None = None,
) -> None:
    """Execute the index stage: update Postgres row and flip tier.

    After this stage completes, the object becomes recallable (tier=hot,
    summary populated, FTS column updated via generated column).

    Args:
        obj_id: ULID of the BrainObject to index.
        source: Source identifier.
        clean_ref: MinIO clean bucket key (or None if clean failed).
        summary: Generated summary (or None).
        entities: Extracted entity list.
        linked_events: Linked Kuzu event IDs.
        market_keys: Matched market keys from Qdrant.
        confidence: Optional confidence score (0..1).
    """
    updates: dict = {}

    if clean_ref is not None:
        updates["clean_ref"] = clean_ref
    if summary is not None:
        updates["summary"] = summary
    if entities:
        updates["entities"] = entities
    if linked_events:
        updates["linked_events"] = linked_events
    if market_keys:
        updates["market_keys"] = market_keys
    if confidence is not None:
        updates["confidence"] = confidence

    # Flip tier to hot so the object becomes recallable
    updates["tier"] = Tier.hot.value

    await store.update_object(obj_id, **updates)

    # Emit ingest event (ladder_rung=7 = index)
    await store.emit_ingest_event(
        object_id=obj_id,
        source=source,
        ladder_rung=7,
        bytes_count=len(summary or ""),
    )

    logger.debug(
        "index: object %s indexed (tier=hot, %d updates)",
        obj_id,
        len(updates),
    )
