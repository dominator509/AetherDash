"""Staleness maintenance job.

SPEC-011 contract (nightly):
    Apply staleness rules per object kind:
    - news:             72h -> stale flag
    - filings/market_description:  stale on market resolution
    - email/note:       never auto-stale
    - event:            stale at event conclusion (stub: 7d)
    - report/transcript:  30d -> stale flag
    - document:         90d -> stale flag (not in spec, sensible default)
    - screenshot:       7d  -> stale flag (not in spec, sensible default)

Staleness is expressed by setting the ``staleness_rule`` field to a
machine-readable value and/or setting ``expires_ts``.
"""

import logging
from datetime import UTC, datetime, timedelta

from server.brain import store as brain_store

logger = logging.getLogger(__name__)

# Staleness thresholds in hours
_STALENESS_RULES: dict[str, int | None] = {
    "news": 72,  # 72h -> stale
    "filing": None,  # market resolution (stub: never auto-stale)
    "market_description": None,  # market resolution (stub: never auto-stale)
    "email": None,  # never auto-stale
    "note": None,  # never auto-stale
    "event": 168,  # 7d (proxy for event conclusion)
    "report": 720,  # 30d
    "transcript": 720,  # 30d
    "document": 2160,  # 90d (sensible default)
    "screenshot": 168,  # 7d (sensible default)
}

_KINDS_NEVER_STALE: frozenset[str] = frozenset({"email", "note"})


async def run_staleness_job() -> dict[str, int]:
    """Run the nightly staleness job.

    Iterates all brain_objects and sets ``staleness_rule`` and/or
    ``expires_ts`` based on the object kind and ingested timestamp.

    Returns:
        Dict with count of objects marked stale: {"stale": count}.
    """
    logger.info("staleness: starting nightly staleness job")

    all_objects = await brain_store.list_objects()
    now = datetime.now(UTC)
    stale_count = 0

    for obj in all_objects:
        kind = obj.kind.value if hasattr(obj.kind, "value") else str(obj.kind)

        # Determine staleness rule for this kind
        hours = _STALENESS_RULES.get(kind)

        if hours is None:
            # Never auto-stale or market-resolution-based (stub: skip)
            if kind in _KINDS_NEVER_STALE:
                # Ensure no stale flag is set
                if obj.staleness_rule is not None:
                    await brain_store.update_object(
                        obj.id, staleness_rule=None, expires_ts=None
                    )
            continue

        # Parse ingested_ts
        try:
            ingested_dt = datetime.fromisoformat(obj.ingested_ts.replace("Z", "+00:00"))
        except (ValueError, AttributeError):
            logger.debug("staleness: cannot parse ingested_ts for %s, skipping", obj.id)
            continue

        expiry = ingested_dt + timedelta(hours=hours)
        is_stale = now >= expiry
        expires_iso = expiry.strftime("%Y-%m-%dT%H:%M:%S.000Z")

        if is_stale:
            # Object is past its staleness threshold
            if obj.staleness_rule != "stale":
                await brain_store.update_object(
                    obj.id,
                    staleness_rule="stale",
                    expires_ts=expires_iso,
                )
                stale_count += 1
                logger.debug("staleness: %s marked stale (kind=%s)", obj.id, kind)
        else:
            # Not stale yet, but set expires_ts so downstream can check
            if obj.expires_ts != expires_iso:
                await brain_store.update_object(
                    obj.id,
                    expires_ts=expires_iso,
                )

    logger.info("staleness: done — %d objects marked stale", stale_count)
    return {"stale": stale_count}
