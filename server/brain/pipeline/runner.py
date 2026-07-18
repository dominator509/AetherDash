"""Pipeline orchestrator — runs all 7 stages sequentially.

Each stage is idempotent: if the output artifact already exists (e.g.
``clean_ref`` is set, ``summary`` is populated), the stage is skipped
so that re-running the pipeline from the start does not repeat work.

Stage failures park the object at the last completed stage with
``current_stage = 'parked:<stage>'`` and a ``parked_reason``. Nothing
is recallable until the final ``index`` stage sets ``tier=hot``.

On resume: if ``current_stage`` starts with ``'parked:'`` the runner
retries that stage; if it is a plain stage name it continues to the
next (the relevant artifact already exists and idempotency will skip it).
"""

import logging
from typing import Any

from server.brain import storage as brain_storage
from server.brain import store as brain_store
from server.brain.pipeline import clean, embed, extract, index, link, summarize

logger = logging.getLogger(__name__)

# Pipeline stage order. Compliance ladder rungs describe the source transport,
# not these processing stages; intake records the source rung exactly once.
_STAGE_ORDER = ["intake", "clean", "summarize", "extract", "link", "embed", "index"]


async def _set_stage(obj_id: str, stage: str, parked_reason: str | None = None) -> None:
    """Persist the current stage for an object.

    Args:
        obj_id: ULID of the object.
        stage: Stage name (e.g. ``'clean'``, ``'parked:link'``).
        parked_reason: Optional error reason when parking.
    """
    updates: dict[str, Any] = {"current_stage": stage}
    if parked_reason is not None:
        updates["parked_reason"] = parked_reason
    await brain_store.update_object(obj_id, **updates)


def _next_stage(current: str) -> str | None:
    """Return the next stage in the pipeline order, or None if at the end."""
    if current not in _STAGE_ORDER:
        return None
    idx = _STAGE_ORDER.index(current)
    if idx + 1 >= len(_STAGE_ORDER):
        return None
    return _STAGE_ORDER[idx + 1]


def _resolve_resume_stage(current_stage: str) -> str | None:
    """Resolve which stage to resume from based on ``current_stage``.

    - ``'parked:<stage>'`` -> retry that stage
    - valid stage name -> return the NEXT stage
    - unknown -> start from ``'clean'`` (intake already done)
    """
    if current_stage.startswith("parked:"):
        return current_stage.split(":", 1)[1]
    return _next_stage(current_stage)


async def run_pipeline(object_id: str) -> None:
    """Run all 7 pipeline stages sequentially for the given object.

    Supports resume: on startup, checks ``current_stage`` to determine
    where to pick up from.  Each successful stage persists its stage
    name.  Failures park the object.

    Args:
        object_id: ULID of the BrainObject to process.
    """
    # Fetched once at the start — idempotency checks use these values.
    obj = await brain_store.get_object(object_id)
    if obj is None:
        logger.warning("run_pipeline: object %s not found — skipping", object_id)
        return

    source = obj.source
    logger.debug(
        "run_pipeline: starting for object %s (kind=%s, stage=%s)",
        object_id,
        obj.kind.value,
        obj.current_stage,
    )

    # ── Resume logic ────────────────────────────────────────────────
    resume_from = _resolve_resume_stage(obj.current_stage)
    if resume_from is None:
        # Already fully indexed (current_stage = 'index') — nothing to do
        logger.debug(
            "run_pipeline: object %s already at terminal stage — skipping", object_id
        )
        return

    # If resuming from a parked stage, clear the parked_reason first
    if obj.current_stage.startswith("parked:"):
        await _set_stage(object_id, resume_from)

    # Set cleaned_text at module level for use in later stages
    cleaned_text: str | None = None

    # ── Stage 2: clean ──────────────────────────────────────────────
    if _STAGE_ORDER.index("clean") >= _STAGE_ORDER.index(resume_from):
        try:
            if obj.clean_ref is None:
                raw_bytes = brain_storage.get_raw(obj.raw_ref)
                cleaned_text, clean_ref = await clean.run(raw_bytes, source)
                await brain_store.update_object(object_id, clean_ref=clean_ref)
                logger.debug("pipeline[2/7 clean]: done for %s", object_id)
                obj.clean_ref = clean_ref
            else:
                clean_bytes = brain_storage.get_clean(obj.clean_ref)
                cleaned_text = clean_bytes.decode("utf-8", errors="replace")
                logger.debug("pipeline[2/7 clean]: skipped (already cleaned)")
            await _set_stage(object_id, "clean")
        except Exception as exc:
            logger.error("pipeline[2/7 clean]: FAILED for %s — %s", object_id, exc)
            await _set_stage(object_id, "parked:clean", str(exc))
            return

    # ── Stage 3: summarize ──────────────────────────────────────────
    if _STAGE_ORDER.index("summarize") >= _STAGE_ORDER.index(resume_from):
        summary_text: str | None = None
        try:
            # Ensure cleaned_text is available (from clean stage or DB fallback)
            if cleaned_text is None and obj.summary is None:
                # Fetch cleaned text if we skipped clean
                raw_bytes = brain_storage.get_raw(obj.raw_ref)
                _ct, clean_ref = await clean.run(raw_bytes, source)
                await brain_store.update_object(object_id, clean_ref=clean_ref)
                obj.clean_ref = clean_ref
                cleaned_bytes = brain_storage.get_clean(clean_ref)
                cleaned_text = cleaned_bytes.decode("utf-8", errors="replace")

            if obj.summary is None:
                summary_text = await summarize.run(cleaned_text or "")
                await brain_store.update_object(object_id, summary=summary_text)
                logger.debug("pipeline[3/7 summarize]: done for %s", object_id)
                obj.summary = summary_text
            else:
                summary_text = obj.summary
                logger.debug("pipeline[3/7 summarize]: skipped (already summarized)")
            await _set_stage(object_id, "summarize")
        except Exception as exc:
            logger.error("pipeline[3/7 summarize]: FAILED for %s — %s", object_id, exc)
            await _set_stage(object_id, "parked:summarize", str(exc))
            return

    entities: list[str] = []
    linked_events: list[str] = []
    market_keys: list[str] = []

    # ── Stage 4: extract ────────────────────────────────────────────
    if _STAGE_ORDER.index("extract") >= _STAGE_ORDER.index(resume_from):
        try:
            existing_entities_list = obj.entities if obj.entities else []
            if not existing_entities_list:
                extract_result = await extract.run(cleaned_text or "")
                entities = extract_result.get("entities", [])
                # Write empty marker: entities=[] means "ran and found nothing"
                await brain_store.update_object(object_id, entities=entities)
                logger.debug("pipeline[4/7 extract]: done for %s", object_id)
            else:
                entities = existing_entities_list
                logger.debug("pipeline[4/7 extract]: skipped (already extracted)")
            await _set_stage(object_id, "extract")
        except Exception as exc:
            logger.error("pipeline[4/7 extract]: FAILED for %s — %s", object_id, exc)
            await _set_stage(object_id, "parked:extract", str(exc))
            return

    # ── Stage 5: link ───────────────────────────────────────────────
    if _STAGE_ORDER.index("link") >= _STAGE_ORDER.index(resume_from):
        try:
            existing_linked = obj.linked_events if obj.linked_events else []
            existing_markets = obj.market_keys if obj.market_keys else []
            if not existing_linked and not existing_markets:
                linked_events_result, market_keys_result = await link.run(
                    summary_text or "", entities, obj
                )
                updates: dict[str, object] = {}
                # Write empty markers: empty lists mean "ran and found nothing"
                updates["linked_events"] = linked_events_result
                updates["market_keys"] = market_keys_result
                await brain_store.update_object(object_id, **updates)
                linked_events = linked_events_result
                market_keys = market_keys_result
                logger.debug("pipeline[5/7 link]: done for %s", object_id)
            else:
                linked_events = existing_linked
                market_keys = existing_markets
                logger.debug("pipeline[5/7 link]: skipped (already linked)")
            await _set_stage(object_id, "link")
        except Exception as exc:
            logger.error("pipeline[5/7 link]: FAILED for %s — %s", object_id, exc)
            await _set_stage(object_id, "parked:link", str(exc))
            return

    # ── Stage 6: embed ──────────────────────────────────────────────
    chunk_count = 0
    if _STAGE_ORDER.index("embed") >= _STAGE_ORDER.index(resume_from):
        try:
            chunk_count = await embed.run(cleaned_text or "", object_id, source)
            logger.debug(
                "pipeline[6/7 embed]: done for %s (%d chunks)", object_id, chunk_count
            )
            await _set_stage(object_id, "embed")
        except Exception as exc:
            logger.error("pipeline[6/7 embed]: FAILED for %s — %s", object_id, exc)
            await _set_stage(object_id, "parked:embed", str(exc))
            return

    # ── Stage 7: index ──────────────────────────────────────────────
    if _STAGE_ORDER.index("index") >= _STAGE_ORDER.index(resume_from):
        try:
            await index.run(
                obj_id=object_id,
                source=source,
                clean_ref=obj.clean_ref,
                summary=summary_text,
                entities=entities,
                linked_events=linked_events,
                market_keys=market_keys,
                confidence=None,
            )
            # After successful index, persist the final stage
            await _set_stage(object_id, "index")
            logger.info(
                "pipeline[7/7 index]: DONE for %s — object recallable", object_id
            )
        except Exception as exc:
            logger.error(
                "pipeline[7/7 index]: FAILED for %s — %s; object NOT recallable",
                object_id,
                exc,
            )
            await _set_stage(object_id, "parked:index", str(exc))
            return


async def run_pipeline_sync(object_id: str) -> None:
    """Synchronous wrapper for ``run_pipeline``.

    For use in contexts where the caller wants a simple await.
    Delegates to ``run_pipeline`` directly.
    """
    await run_pipeline(object_id)
