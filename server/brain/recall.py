"""Recall v1 — hybrid RRF retrieval for Brain objects.

Algorithm: Reciprocal Rank Fusion (RRF) over:
1. Qdrant vector search (brain_chunks collection)
2. Postgres FTS (ts_vector on brain_objects)

Deterministic per INV-1 (no LLM inside recall).  The embedding used for
Qdrant search is a stub vector (seeded random) until EP-202 replaces it
with a real embedding model.

Typical usage::

    from server.brain.recall import recall

    results = await recall("market conditions", k=24, filters={
        "kind": "news",
        "trust": "medium",
        "market_keys": ["POLYMARKET:12345"],
    })
"""

import asyncio
import logging
import math
import os
import time
from dataclasses import dataclass, replace
from datetime import UTC, datetime
from typing import Any

logger = logging.getLogger(__name__)

# ── Constants ───────────────────────────────────────────────────────────────

_RRF_CONSTANT = 60
_GRAPH_RRF_WEIGHT = 0.15
_DECAY_HALF_LIFE_HOURS: dict[str, float | None] = {
    "news": 72.0,
    "filing": None,
    "market_description": None,
    "email": None,
    "note": None,
    "event": 168.0,
    "report": 720.0,
    "transcript": 720.0,
    "document": 2160.0,
    "screenshot": 168.0,
}

# ── Domain types ────────────────────────────────────────────────────────────


@dataclass
class ScoredRef:
    """A single recall result with fusion score and per-source ranks."""

    object_id: str
    provenance_hash: str
    score: float  # RRF fusion score
    qdrant_rank: int | None = None
    fts_rank: int | None = None
    graph_rank: int | None = None
    decay_weight: float | None = None
    reliability_weight: float | None = None
    recall_path: str = "v1"
    rerank_status: str | None = None
    rerank_score: float | None = None


@dataclass(frozen=True)
class RecallMetadata:
    kind: str
    ingested_ts: datetime
    source_reliability: float = 0.5


# ── Qdrant search ──────────────────────────────────────────────────────────


_QDrant_CLIENT: object | None = None


def _get_qdrant_client():
    """Get a cached ``QdrantClient`` instance.

    The client is created once and reused across calls to avoid the
    connection-setup overhead (~150ms) on every recall request.
    """
    global _QDrant_CLIENT  # noqa: PLW0603

    if _QDrant_CLIENT is not None:
        return _QDrant_CLIENT

    from qdrant_client import QdrantClient  # noqa: PLC0415

    _qdrant_url = os.environ.get("AETHER_QDRANT__URL", "http://localhost:6333")
    _QDrant_CLIENT = QdrantClient(url=_qdrant_url)
    return _QDrant_CLIENT


async def _qdrant_search(
    query: str,
    k: int,
) -> list[dict[str, Any]]:
    """Search Qdrant's ``brain_chunks`` collection and group by object_id.

    Args:
        query: Search query text. Empty query returns empty list.
        k: Target number of unique objects to return (over-fetches chunks
           and deduplicates by object_id).

    Returns:
        List of dicts with ``object_id`` and ``score`` keys, sorted by
        score descending. Returns empty list if Qdrant is unreachable
        or query is empty.
    """
    if not query.strip():
        return []

    try:
        from server.brain.pipeline.embed import generate_embedding  # noqa: PLC0415

        client = _get_qdrant_client()
        query_vector = await generate_embedding(query)

        # Over-fetch chunks to account for deduplication across chunks
        # that belong to the same object.
        search_k = k * 4
        # qdrant-client >= 1.9 API: query_points replaces the older search().
        result = client.query_points(
            collection_name="brain_chunks",
            query=query_vector,
            limit=search_k,
        )
        hits = result.points if hasattr(result, "points") else result

        # Group by object_id, keep the maximum score per object
        object_scores: dict[str, float] = {}
        for hit in hits:
            payload = hit.payload if hasattr(hit, "payload") else {}
            oid = payload.get("object_id", "")
            if oid:
                score = hit.score
                if oid not in object_scores or score > object_scores[oid]:
                    object_scores[oid] = score

        # Sort by score descending, cap at k
        sorted_objs = sorted(object_scores.items(), key=lambda x: x[1], reverse=True)[
            :k
        ]

        return [{"object_id": oid, "score": score} for oid, score in sorted_objs]

    except Exception as exc:
        logger.warning("Qdrant search failed (%s) — returning empty results", exc)
        return []


# ── Postgres FTS search ────────────────────────────────────────────────────


async def _fts_search(
    query: str,
    k: int,
    filters: dict | None = None,
) -> list[dict[str, Any]]:
    """Search ``brain_objects`` via Postgres full-text search with filters.

    Uses ``ts_rank`` + ``plainto_tsquery`` on the generated ``fts_vector`` column
    (which is ``to_tsvector('english', coalesce(summary, '') || ' ' || coalesce(kind, ''))``).

    Supported filter keys (all optional):

        ``kind`` (str | list[str])
            Object kind(s) to include.
        ``trust`` (str)
            Minimum ``TrustLevel`` (``"low"`` / ``"medium"`` / ``"high"``).
        ``market_keys`` (list[str])
            Require at least one matching market key.
        ``tier_exclude`` (str)
            Tier to exclude (default ``"cold"``). Pass ``None`` to include all.
        ``time_from`` (str)
            ISO-8601 datetime; ``ingested_ts >=``.
        ``time_to`` (str)
            ISO-8601 datetime; ``ingested_ts <=``.

    Args:
        query: Search query text.
        k: Maximum number of results.
        filters: Optional filter dict.

    Returns:
        List of dicts with ``object_id``, ``provenance_hash``, and ``score`` keys,
        sorted by FTS rank descending. Empty list for empty query or on error.
    """
    if not query.strip():
        return []

    from server.brain import store as brain_store  # noqa: PLC0415

    pool = await brain_store.get_pool()
    filters = filters or {}

    conditions: list[str] = ["fts_vector @@ plainto_tsquery($1::text)"]
    params: list[Any] = [query]
    idx = 2

    # --- tier exclusion (default: exclude "cold") ---
    tier_exclude = filters.get("tier_exclude", "cold")
    if tier_exclude is not None:
        conditions.append(f"tier != ${idx}")
        params.append(tier_exclude)
        idx += 1

    # --- kind filter (single or list) ---
    kind_val = filters.get("kind")
    if kind_val is not None:
        kinds = [kind_val] if isinstance(kind_val, str) else list(kind_val)
        if kinds:
            placeholders = ", ".join(f"${idx + i}" for i in range(len(kinds)))
            conditions.append(f"kind IN ({placeholders})")
            params.extend(kinds)
            idx += len(kinds)

    # --- trust minimum ---
    trust_min = filters.get("trust")
    if trust_min is not None:
        from server.brain.models import TrustLevel, trust_to_numeric  # noqa: PLC0415

        numeric = trust_to_numeric(TrustLevel(trust_min))
        conditions.append(f"trust >= ${idx}::numeric")
        params.append(str(numeric))
        idx += 1

    # --- market_keys (JSONB ?| = any-of) ---
    market_keys = filters.get("market_keys", [])
    if market_keys:
        placeholders = ", ".join(f"${idx + i}" for i in range(len(market_keys)))
        conditions.append(f"market_keys ?| ARRAY[{placeholders}]")
        params.extend(market_keys)
        idx += len(market_keys)

    # --- time window ---
    time_from = filters.get("time_from")
    if time_from is not None:
        conditions.append(f"ingested_ts >= ${idx}::timestamptz")
        params.append(time_from)
        idx += 1

    time_to = filters.get("time_to")
    if time_to is not None:
        conditions.append(f"ingested_ts <= ${idx}::timestamptz")
        params.append(time_to)
        idx += 1

    where_clause = " AND ".join(conditions)

    sql = f"""
        SELECT id, provenance_hash,
               ts_rank(fts_vector, plainto_tsquery($1::text)) AS score
        FROM brain_objects
        WHERE {where_clause}
        ORDER BY score DESC
        LIMIT ${idx}
    """
    params.append(k)

    try:
        async with pool.acquire() as conn:
            rows = await conn.fetch(sql, *params)

        return [
            {
                "object_id": row["id"],
                "provenance_hash": row["provenance_hash"],
                "score": float(row["score"]) if row["score"] is not None else 0.0,
            }
            for row in rows
        ]
    except Exception as exc:
        logger.warning("FTS search failed (%s) — returning empty results", exc)
        return []


# ── RRF fusion ─────────────────────────────────────────────────────────────


def _rrf_fuse(
    qdrant_results: list[dict[str, Any]],
    fts_results: list[dict[str, Any]],
    k: int,
) -> list[ScoredRef]:
    """Fuse two ranked result lists using Reciprocal Rank Fusion.

    RRF formula::

        score(obj) = SUM over sources S  of  1 / (rank_S(obj) + RRF_CONSTANT)

    where ``rank_S(obj)`` is the 1-based rank of the object in source S.
    An object absent from a source contributes 0 from that source.

    Args:
        qdrant_results: Results from Qdrant, each with ``object_id``.
        fts_results: Results from FTS, each with ``object_id``,
            ``provenance_hash``, and ``score``.
        k: Maximum number of results to return.

    Returns:
        List of ``ScoredRef`` sorted by descending fusion score.
        Ties broken by object_id for determinism.
    """
    # Build 1-based rank lookups
    q_rank: dict[str, int] = {
        r["object_id"]: i + 1 for i, r in enumerate(qdrant_results)
    }
    f_rank: dict[str, int] = {r["object_id"]: i + 1 for i, r in enumerate(fts_results)}

    # Collect provenance hashes from FTS results (Qdrant results don't carry them).
    # Use .get() in case caller passes dicts without the key (e.g., unit tests).
    provenance: dict[str, str] = {
        r["object_id"]: r.get("provenance_hash", "") for r in fts_results
    }

    all_ids: set[str] = set(q_rank) | set(f_rank)

    fused: list[ScoredRef] = []
    for oid in all_ids:
        fusion_score = 0.0
        qr = q_rank.get(oid)
        fr = f_rank.get(oid)
        if qr is not None:
            fusion_score += 1.0 / (qr + _RRF_CONSTANT)
        if fr is not None:
            fusion_score += 1.0 / (fr + _RRF_CONSTANT)

        fused.append(
            ScoredRef(
                object_id=oid,
                provenance_hash=provenance.get(oid, ""),
                score=fusion_score,
                qdrant_rank=qr,
                fts_rank=fr,
            )
        )

    # Sort descending by fusion score, tiebreak by object_id
    fused.sort(key=lambda s: (-s.score, s.object_id))
    return fused[:k]


def _rrf_fuse_with_graph(
    qdrant_results: list[dict[str, Any]],
    fts_results: list[dict[str, Any]],
    graph_results: list[dict[str, Any]],
    k: int,
) -> list[ScoredRef]:
    """Fuse v1 rankings plus deterministic one-hop graph candidates."""
    fused = _rrf_fuse(
        qdrant_results,
        fts_results,
        len(
            {
                result["object_id"]
                for result in [*qdrant_results, *fts_results, *graph_results]
            }
        ),
    )
    by_id = {ref.object_id: ref for ref in fused}
    graph_rank = {
        result["object_id"]: rank for rank, result in enumerate(graph_results, start=1)
    }
    provenance = {
        result["object_id"]: result.get("provenance_hash", "")
        for result in graph_results
    }
    for object_id, rank in graph_rank.items():
        ref = by_id.get(object_id)
        if ref is None:
            ref = ScoredRef(
                object_id=object_id,
                provenance_hash=provenance.get(object_id, ""),
                score=0.0,
            )
            by_id[object_id] = ref
        ref.score += _GRAPH_RRF_WEIGHT / (rank + _RRF_CONSTANT)
        ref.graph_rank = rank
    ranked = sorted(by_id.values(), key=lambda ref: (-ref.score, ref.object_id))
    return ranked[:k]


def _decay_weight(kind: str, ingested_ts: datetime, now: datetime) -> float:
    half_life = _DECAY_HALF_LIFE_HOURS.get(kind, 720.0)
    if half_life is None:
        return 1.0
    age_hours = max(0.0, (now - ingested_ts).total_seconds() / 3600.0)
    return math.pow(2.0, -age_hours / half_life)


def _apply_decay_and_reliability(
    refs: list[ScoredRef],
    metadata: dict[str, RecallMetadata],
    *,
    now: datetime | None = None,
) -> list[ScoredRef]:
    """Apply monotone age decay and bounded source-reliability weighting."""
    resolved_now = now or datetime.now(UTC)
    for ref in refs:
        item = metadata.get(ref.object_id)
        if item is None:
            decay = 1.0
            reliability = 0.5
        else:
            reliability = min(1.0, max(0.0, item.source_reliability))
            ingested_ts = item.ingested_ts
            if ingested_ts.tzinfo is None:
                ingested_ts = ingested_ts.replace(tzinfo=UTC)
            decay = _decay_weight(item.kind, ingested_ts, resolved_now)
        reliability_weight = 0.5 + reliability
        ref.score *= decay * reliability_weight
        ref.decay_weight = decay
        ref.reliability_weight = reliability_weight
    refs.sort(key=lambda ref: (-ref.score, ref.object_id))
    return refs


async def _fetch_recall_metadata(object_ids: list[str]) -> dict[str, RecallMetadata]:
    if not object_ids:
        return {}
    from server.brain import store as brain_store  # noqa: PLC0415
    from server.brain.graph import source_reliabilities  # noqa: PLC0415
    from server.brain.pipeline.link import _init_kuzu  # noqa: PLC0415

    pool = await brain_store.get_pool()
    async with pool.acquire() as connection:
        rows = await connection.fetch(
            "SELECT id,kind,ingested_ts,source FROM brain_objects "
            "WHERE id = ANY($1::text[])",
            list(dict.fromkeys(object_ids)),
        )
    sources = [str(row["source"]) for row in rows]
    try:
        kuzu_connection = _init_kuzu()
        reliability = (
            source_reliabilities(kuzu_connection, sources)
            if kuzu_connection is not None
            else {}
        )
    except Exception as exc:
        logger.warning(
            "Source reliability lookup failed (%s) — using neutral weights",
            type(exc).__name__,
        )
        reliability = {}
    return {
        str(row["id"]): RecallMetadata(
            kind=str(row["kind"]),
            ingested_ts=row["ingested_ts"],
            source_reliability=reliability.get(str(row["source"]), 0.5),
        )
        for row in rows
    }


# ── Filter Qdrant results via Postgres ─────────────────────────────────────


async def _filter_qdrant_results(
    qdrant_results: list[dict[str, Any]],
    filters: dict[str, Any],
) -> tuple[list[dict[str, Any]], dict[str, str]]:
    """Post-filter Qdrant results against Postgres and fetch provenance hashes.

    Since Qdrant chunk payloads don't carry ``kind``, ``trust``, ``tier``, or
    ``market_keys``, we must verify each candidate object passes the active
    filters by looking up its Postgres row.  This also retrieves
    ``provenance_hash`` for Qdrant-only results.

    Args:
        qdrant_results: Raw Qdrant results (``object_id``, ``score``).
        filters: Filter dict (same keys as ``_fts_search``).

    Returns:
        Tuple of ``(filtered_results, provenance_map)`` where
        ``provenance_map`` maps ``object_id -> provenance_hash`` for the
        filtered objects.
    """
    if not qdrant_results:
        return [], {}

    from server.brain import store as brain_store  # noqa: PLC0415

    pool = await brain_store.get_pool()

    obj_ids = [r["object_id"] for r in qdrant_results]

    conditions: list[str] = ["id = ANY($1::text[])"]
    params: list[Any] = [obj_ids]
    idx = 2

    # --- tier exclusion (default: exclude "cold") ---
    tier_exclude = filters.get("tier_exclude", "cold")
    if tier_exclude is not None:
        conditions.append(f"tier != ${idx}")
        params.append(tier_exclude)
        idx += 1

    # --- kind filter ---
    kind_val = filters.get("kind")
    if kind_val is not None:
        kinds = [kind_val] if isinstance(kind_val, str) else list(kind_val)
        if kinds:
            placeholders = ", ".join(f"${idx + i}" for i in range(len(kinds)))
            conditions.append(f"kind IN ({placeholders})")
            params.extend(kinds)
            idx += len(kinds)

    # --- trust minimum ---
    trust_min = filters.get("trust")
    if trust_min is not None:
        from server.brain.models import TrustLevel, trust_to_numeric  # noqa: PLC0415

        numeric = trust_to_numeric(TrustLevel(trust_min))
        conditions.append(f"trust >= ${idx}::numeric")
        params.append(str(numeric))
        idx += 1

    # --- market_keys ---
    market_keys = filters.get("market_keys", [])
    if market_keys:
        placeholders = ", ".join(f"${idx + i}" for i in range(len(market_keys)))
        conditions.append(f"market_keys ?| ARRAY[{placeholders}]")
        params.extend(market_keys)
        idx += len(market_keys)

    # --- time window ---
    time_from = filters.get("time_from")
    if time_from is not None:
        conditions.append(f"ingested_ts >= ${idx}::timestamptz")
        params.append(time_from)
        idx += 1

    time_to = filters.get("time_to")
    if time_to is not None:
        conditions.append(f"ingested_ts <= ${idx}::timestamptz")
        params.append(time_to)
        idx += 1

    where_clause = " AND ".join(conditions)
    sql = f"SELECT id, provenance_hash FROM brain_objects WHERE {where_clause}"

    try:
        async with pool.acquire() as conn:
            rows = await conn.fetch(sql, *params)
    except Exception as exc:
        logger.warning("Qdrant filter lookup failed (%s) — returning empty", exc)
        return [], {}

    valid_ids = {row["id"] for row in rows}
    provenance_map = {row["id"]: row["provenance_hash"] for row in rows}

    filtered = [r for r in qdrant_results if r["object_id"] in valid_ids]
    return filtered, provenance_map


# ── Main entry point ───────────────────────────────────────────────────────


async def recall_v1(
    query: str,
    k: int = 24,
    filters: dict | None = None,
) -> list[ScoredRef]:
    """Hybrid RRF recall — fetches from Qdrant and Postgres FTS, fuses results.

    This is the primary recall entry point used by the service layer and gRPC
    handler.  It is deterministic per INV-1 (no LLM call).

    Args:
        query: The search query string.
        k: Number of results to return (default 24).
        filters: Optional filter dict. See ``_fts_search`` for supported keys.

    Returns:
        List of ``ScoredRef`` sorted by fusion score descending.
        Returns empty list for empty query or errors.
    """
    if not query.strip():
        return []

    filters = filters or {}

    # 1. Qdrant vector search (brain_chunks)
    qdrant_raw = await _qdrant_search(query, k)

    # 2. Post-filter Qdrant results via Postgres (Qdrant chunks don't carry
    #    kind, trust, tier, or market_keys in their payload)
    provenance_map: dict[str, str] = {}
    if qdrant_raw:
        qdrant_results, provenance_map = await _filter_qdrant_results(
            qdrant_raw, filters
        )
    else:
        qdrant_results = []

    # 3. Postgres FTS (brain_objects) — filters baked into the SQL WHERE
    fts_results = await _fts_search(query, k, filters)

    # 4. Merge provenance hashes from FTS into the map for any objects that
    #    also appeared in Qdrant but didn't get one from the filter step
    for r in fts_results:
        if r["object_id"] not in provenance_map:
            provenance_map[r["object_id"]] = r["provenance_hash"]

    # 5. RRF fusion – the fusion function reads provenance hashes from the
    #    second argument, so we enrich them before fusion
    for r in fts_results:
        r.setdefault("provenance_hash", provenance_map.get(r["object_id"], ""))

    fused = _rrf_fuse(qdrant_results, fts_results, k)

    # Ensure every ScoredRef has its provenance_hash filled
    for ref in fused:
        if not ref.provenance_hash:
            ref.provenance_hash = provenance_map.get(ref.object_id, "")

    return fused


async def _fetch_recall_documents(object_ids: list[str]) -> dict[str, str]:
    if not object_ids:
        return {}
    from server.brain import store as brain_store  # noqa: PLC0415

    pool = await brain_store.get_pool()
    async with pool.acquire() as connection:
        rows = await connection.fetch(
            "SELECT id,COALESCE(summary,kind) AS document FROM brain_objects "
            "WHERE id = ANY($1::text[])",
            list(dict.fromkeys(object_ids)),
        )
    return {str(row["id"]): str(row["document"]) for row in rows}


async def _enhance_v2(
    query: str,
    v1_refs: list[ScoredRef],
    filters: dict[str, Any],
    *,
    k: int,
) -> list[ScoredRef]:
    from server.brain.graph import expand_connection  # noqa: PLC0415
    from server.brain.pipeline.link import _init_kuzu  # noqa: PLC0415

    candidate_k = max(k, 24)
    seed_ids = [ref.object_id for ref in v1_refs]
    try:
        kuzu_connection = _init_kuzu()
        graph_candidates = (
            expand_connection(kuzu_connection, seed_ids, limit=candidate_k)
            if kuzu_connection is not None
            else []
        )
    except Exception as exc:
        logger.warning(
            "Graph expansion failed (%s) — continuing without graph neighbors",
            type(exc).__name__,
        )
        graph_candidates = []

    graph_raw = [
        {"object_id": candidate.object_id, "shared_edges": candidate.shared_edges}
        for candidate in graph_candidates
    ]
    graph_results, graph_provenance = await _filter_qdrant_results(graph_raw, filters)
    for result in graph_results:
        result["provenance_hash"] = graph_provenance.get(result["object_id"], "")

    qdrant_results = [
        {"object_id": ref.object_id, "score": ref.score}
        for ref in sorted(
            (ref for ref in v1_refs if ref.qdrant_rank is not None),
            key=lambda ref: ref.qdrant_rank or 0,
        )
    ]
    fts_results = [
        {
            "object_id": ref.object_id,
            "provenance_hash": ref.provenance_hash,
            "score": ref.score,
        }
        for ref in sorted(
            (ref for ref in v1_refs if ref.fts_rank is not None),
            key=lambda ref: ref.fts_rank or 0,
        )
    ]
    enhanced = _rrf_fuse_with_graph(
        qdrant_results, fts_results, graph_results, candidate_k
    )
    metadata = await _fetch_recall_metadata([ref.object_id for ref in enhanced])
    enhanced = _apply_decay_and_reliability(enhanced, metadata)[:k]
    for ref in enhanced:
        ref.recall_path = "v2"

    if os.environ.get("AETHER_BRAIN__RECALL_RERANK", "0") != "1":
        for ref in enhanced:
            ref.rerank_status = "disabled"
        return enhanced

    from server.brain.rerank import (  # noqa: PLC0415
        RouterCrossEncoder,
        rerank_with_budget,
    )

    documents = await _fetch_recall_documents([ref.object_id for ref in enhanced])
    rerank_timeout_ms = min(
        25.0,
        float(os.environ.get("AETHER_BRAIN__RECALL_RERANK_TIMEOUT_MS", "25")),
    )
    outcome = await rerank_with_budget(
        query,
        enhanced,
        documents,
        RouterCrossEncoder(),
        top_m=min(12, len(enhanced)),
        timeout_ms=rerank_timeout_ms,
    )
    for ref in outcome.refs:
        ref.rerank_status = outcome.reason
    return outcome.refs


async def recall(
    query: str,
    k: int = 24,
    filters: dict | None = None,
) -> list[ScoredRef]:
    """Recall v2 by default, with an outer budget breaker to v1 results."""
    started = time.perf_counter()
    candidate_k = max(k, 24)
    v1_refs = await recall_v1(query, candidate_k, filters)
    if os.environ.get("AETHER_BRAIN__RECALL_V2", "1") != "1" or not v1_refs:
        return v1_refs[:k]

    budget_ms = min(
        100.0, max(1.0, float(os.environ.get("AETHER_BRAIN__RECALL_BUDGET_MS", "100")))
    )
    remaining_s = budget_ms / 1_000 - (time.perf_counter() - started) - 0.030
    if remaining_s <= 0:
        fallback = [replace(ref) for ref in v1_refs[:k]]
        for ref in fallback:
            ref.recall_path = "v1_budget_fallback"
        return fallback
    fallback = [replace(ref) for ref in v1_refs[:k]]
    try:
        return await asyncio.wait_for(
            _enhance_v2(query, v1_refs, filters or {}, k=k), timeout=remaining_s
        )
    except TimeoutError:
        # asyncpg completes protocol cancellation on a follow-up loop turn.
        # This stays inside the 30 ms reserve and avoids leaking cleanup tasks.
        await asyncio.sleep(0.020)
        for ref in fallback:
            ref.recall_path = "v1_budget_fallback"
        return fallback
    except Exception as exc:
        logger.warning(
            "Recall v2 failed (%s) — returning v1 fallback", type(exc).__name__
        )
        for ref in fallback:
            ref.recall_path = "v1_error_fallback"
        return fallback
