"""Tiering maintenance job.

SPEC-011 contract (nightly):
    1. HOT   = accessed <= 7d OR linked to open markets -> keep hot
    2. WARM  = neither hot nor cold, < 90d
    3. COLD  = rest: drop Qdrant vectors, keep summary + MinIO
    4. Resolved-market sweep: objects linked only to resolved markets
       get archival roll-up (one synthesis per market) and go cold

All market-resolution checks are stubbed for EP-201 (all markets assumed open).
Qdrant vector deletion is now **active** (no longer a stub).
"""

import logging
import os
from datetime import UTC, datetime

from server.brain import store as brain_store
from server.brain.models import (
    BrainObject,
    ObjectKind,
    Origin,
    Tier,
    TrustLevel,
    compute_provenance_hash,
    now_iso,
)

logger = logging.getLogger(__name__)

_HOT_ACCESS_WINDOW_DAYS = 7
_WARM_AGE_DAYS = 90

# Qdrant collection name (must match server.brain.pipeline.embed._QDRANT_COLLECTION_CHUNKS)
_QDRANT_COLLECTION_CHUNKS = "brain_chunks"

# Comma-separated list of market_keys known to be resolved.
# When empty (the EP-201 default) the resolved-market sweep is a no-op.
_AETHER_RESOLVED_MARKETS = os.environ.get("AETHER_TIERING__RESOLVED_MARKETS", "")


async def run_tiering_job() -> dict[str, int]:
    """Run the nightly tiering job.

    Returns:
        Dict with counts of objects moved: {tier: count}.
    """
    logger.info("tiering: starting nightly tiering job")

    all_objects = await brain_store.list_objects()
    now = datetime.now(UTC)
    moves: dict[str, int] = {"to_hot": 0, "to_warm": 0, "to_cold": 0}

    for obj in all_objects:
        current_tier = obj.tier.value if hasattr(obj.tier, "value") else str(obj.tier)
        target_tier = _compute_target_tier(obj, now)

        if target_tier != current_tier:
            await brain_store.update_object(obj.id, tier=target_tier)
            moves[f"to_{target_tier}"] = moves.get(f"to_{target_tier}", 0) + 1
            logger.debug(
                "tiering: %s moved from %s -> %s",
                obj.id,
                current_tier,
                target_tier,
            )

            # If moved to cold, drop Qdrant vectors
            if target_tier == "cold":
                await _drop_qdrant_vectors(obj.id)

    # Resolved-market archival roll-up
    rollup_moves = await _run_resolved_market_rollup(all_objects, now)
    for key, count in rollup_moves.items():
        moves[key] = moves.get(key, 0) + count

    total_moved = sum(moves.values())
    logger.info("tiering: done — %d objects re-tiered (%s)", total_moved, moves)
    return moves


def _compute_target_tier(obj: object, now: datetime) -> str:
    """Compute the target tier for a BrainObject based on access + age.

    Uses ``ingested_ts`` as a proxy for last-access time.

    Stub:
    - All markets are assumed open (no resolved-market sweep).
    - Access recency is estimated from the object's timestamp fields.
    """
    from server.brain.models import BrainObject  # noqa: PLC0415

    assert isinstance(obj, BrainObject)

    # HOT: accessed within 7d or linked to open markets
    # Access recency proxy: use ingested_ts
    ingested_str = obj.ingested_ts
    try:
        ingested_dt = datetime.fromisoformat(ingested_str.replace("Z", "+00:00"))
    except (ValueError, AttributeError):
        ingested_dt = now

    days_since_ingested = (now - ingested_dt).total_seconds() / 86400.0

    # If ingested within the hot window -> keep hot
    if days_since_ingested <= _HOT_ACCESS_WINDOW_DAYS:
        return "hot"

    # Linked to open markets -> keep hot (stub: all markets open)
    # In production, check market resolution status via Kuzu/Qdrant.
    # For EP-201 stub, any object with market_keys stays hot.
    if obj.market_keys:
        return "hot"

    # WARM: ingested < 90d and not hot
    if days_since_ingested < _WARM_AGE_DAYS:
        return "warm"

    # COLD: everything else
    return "cold"


# ── Qdrant vector deletion ────────────────────────────────────────────────


def _get_qdrant_client():
    """Get a ``QdrantClient`` instance."""
    from qdrant_client import QdrantClient  # noqa: PLC0415

    return QdrantClient(
        url=os.environ.get("AETHER_QDRANT__URL", "http://localhost:6333")
    )


async def _drop_qdrant_vectors(obj_id: str) -> None:
    """Drop Qdrant vectors for a cold object.

    Deletes all points from the ``brain_chunks`` collection where the
    payload ``object_id`` matches the given object ULID.
    """
    try:
        from qdrant_client.models import (  # noqa: PLC0415
            FieldCondition,
            Filter,
            MatchValue,
        )

        client = _get_qdrant_client()
        client.delete(
            collection_name=_QDRANT_COLLECTION_CHUNKS,
            points_selector=Filter(
                must=[
                    FieldCondition(
                        key="object_id",
                        match=MatchValue(value=obj_id),
                    )
                ]
            ),
        )
        logger.debug("tiering: dropped Qdrant vectors for cold object %s", obj_id)
    except Exception as exc:
        logger.warning(
            "tiering: failed to drop Qdrant vectors for cold object %s: %s",
            obj_id,
            exc,
        )


# ── Resolved-market archival roll-up ──────────────────────────────────────


def _parse_resolved_markets() -> set[str]:
    """Parse the comma-separated ``AETHER_TIERING__RESOLVED_MARKETS`` env var.

    Returns:
        Set of market_keys known to be resolved. Empty set when the env var
        is unset (the EP-201 default, making the sweep a no-op).
    """
    if not _AETHER_RESOLVED_MARKETS:
        return set()
    return {m.strip() for m in _AETHER_RESOLVED_MARKETS.split(",") if m.strip()}


def _market_keys_are_all_resolved(
    market_keys: list[str],
    resolved_set: set[str],
) -> bool:
    """Check if *all* market_keys on an object are in the resolved set.

    An object with no market_keys is NOT considered resolved.
    """
    if not market_keys:
        return False
    return all(mk in resolved_set for mk in market_keys)


async def _run_resolved_market_rollup(
    all_objects: list[BrainObject],
    now: datetime,
) -> dict[str, int]:
    """Run the resolved-market archival roll-up sweep.

    Groups objects by market_key, creates a synthesis BrainObject for each
    resolved market, and sets the originals to cold.

    Returns:
        Dict of move counts (``to_cold``, ``rollup_created``).
    """
    resolved_set = _parse_resolved_markets()
    if not resolved_set:
        logger.debug(
            "tiering: resolved-market sweep skipped (no resolved markets configured)"
        )
        return {}

    moves: dict[str, int] = {}
    synthesis_objects: list[BrainObject] = []

    # Group objects by market_key (an object can appear under multiple keys)
    market_groups: dict[str, list[BrainObject]] = {}
    for obj in all_objects:
        if not obj.market_keys:
            continue
        for mk in obj.market_keys:
            if mk in resolved_set:
                market_groups.setdefault(mk, []).append(obj)

    if not market_groups:
        return {}

    # For each resolved market, create a synthesis object
    for market, objects_for_market in market_groups.items():
        summaries = [
            o.summary or "(no summary)" for o in objects_for_market if o.summary
        ]
        if not summaries:
            # No summarised content to roll up -- just set originals to cold
            for obj in objects_for_market:
                await brain_store.update_object(obj.id, tier=Tier.cold.value)
                await _drop_qdrant_vectors(obj.id)
                moves["to_cold"] = moves.get("to_cold", 0) + 1
            continue

        synthesis_text = f"=== Archival roll-up for market {market} ===\n\n"
        synthesis_text += "\n\n".join(
            f"--- Object {i + 1} ---\n{s}" for i, s in enumerate(summaries)
        )

        ingested_ts = now_iso()
        synthesis_obj = BrainObject(
            id=str(__import__("ulid").new()),
            kind=ObjectKind.report,
            source=f"tiering/rollup/{market}",
            origin=Origin.system,
            trust=TrustLevel.medium,
            ingested_ts=ingested_ts,
            provenance_hash=compute_provenance_hash(
                source=f"tiering/rollup/{market}",
                raw_sha256=__import__("hashlib")
                .sha256(synthesis_text.encode())
                .hexdigest(),
                ingested_ts=ingested_ts,
            ),
            tier=Tier.cold,  # roll-ups are cold by definition
            summary=f"Archival roll-up for market {market} ({len(objects_for_market)} objects)",
            market_keys=[market],
            entities=list({e for o in objects_for_market for e in (o.entities or [])}),
        )

        await brain_store.insert_object(synthesis_obj)
        synthesis_objects.append(synthesis_obj)
        moves["rollup_created"] = moves.get("rollup_created", 0) + 1

        # Set originals to cold
        for obj in objects_for_market:
            current_tier = (
                obj.tier.value if hasattr(obj.tier, "value") else str(obj.tier)
            )
            if current_tier != Tier.cold.value:
                await brain_store.update_object(obj.id, tier=Tier.cold.value)
                await _drop_qdrant_vectors(obj.id)
                moves["to_cold"] = moves.get("to_cold", 0) + 1

    if synthesis_objects:
        logger.info(
            "tiering: resolved-market roll-up created %d synthesis objects",
            len(synthesis_objects),
        )

    return moves
