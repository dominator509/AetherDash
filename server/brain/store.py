"""Postgres storage layer for Brain objects.

Uses the ``brain_objects`` table (migration 0015 + 0020).
"""

import json
import logging
import os
from datetime import UTC, datetime
from typing import Any

import asyncpg

from server.brain.models import (
    BrainObject,
    BrainRef,
    ObjectKind,
    Origin,
    Tier,
    numeric_to_trust,
    trust_to_numeric,
)

logger = logging.getLogger(__name__)

_DATABASE_URL = os.environ.get(
    "DATABASE_URL", "postgres://aether:aether@localhost:5432/aether"
)

_pool: asyncpg.Pool | None = None


async def get_pool() -> asyncpg.Pool:
    """Get or create the connection pool."""
    global _pool  # noqa: PLW0603
    if _pool is None:
        _pool = await asyncpg.create_pool(
            _DATABASE_URL,
            min_size=1,
            max_size=5,
        )
    return _pool


async def close_pool() -> None:
    """Close the connection pool."""
    global _pool  # noqa: PLW0603
    if _pool is not None:
        await _pool.close()
        _pool = None


def _iso_to_dt(iso_str: str | None) -> datetime | None:
    """Convert an ISO 8601 string to a ``datetime`` object.

    asyncpg requires ``datetime`` objects for ``TIMESTAMPTZ`` columns.
    """
    if iso_str is None:
        return None
    # Handle trailing 'Z' -> +00:00
    s = iso_str.replace("Z", "+00:00")
    return datetime.fromisoformat(s)


async def insert_object(
    obj: BrainObject,
    on_conflict_do_nothing: bool = False,
) -> str | None:
    """Insert a BrainObject into the ``brain_objects`` table.

    Args:
        obj: The BrainObject to insert.
        on_conflict_do_nothing: If True, uses ON CONFLICT DO NOTHING
            on the content-identity unique index, returning None when
            a conflicting row exists.

    Returns:
        The id of the inserted row, or None if on_conflict_do_nothing
        was True and the row already exists.
    """
    pool = await get_pool()
    conflict_clause = ""
    returning_clause = ""
    if on_conflict_do_nothing:
        # Do not name the new content index here: a targetless conflict clause
        # remains compatible while migration 0023 rolls out and handles both
        # the provenance and source/content unique constraints afterwards.
        conflict_clause = " ON CONFLICT DO NOTHING"
        returning_clause = " RETURNING id"
    async with pool.acquire() as conn:
        row = await conn.fetchrow(
            f"""
            INSERT INTO brain_objects (
                id, kind, source, origin, trust,
                provenance_hash, minio_raw_ref, minio_clean_ref,
                summary, staleness_rule, expires_ts, tier,
                author_or_publisher, published_ts, ingested_ts,
                url_or_ref, raw_sha256, entities, linked_events,
                market_keys, confidence,
                current_stage, parked_reason
            ) VALUES (
                $1, $2, $3, $4, $5,
                $6, $7, $8,
                $9, $10, $11, $12,
                $13, $14, $15,
                $16, $17, $18::jsonb, $19::jsonb,
                $20::jsonb, $21,
                $22, $23
            )
            {conflict_clause}
            {returning_clause}
            """,
            obj.id,
            obj.kind.value,
            obj.source,
            obj.origin.value,
            str(trust_to_numeric(obj.trust)),
            obj.provenance_hash,
            obj.raw_ref,
            obj.clean_ref,
            obj.summary,
            obj.staleness_rule,
            _iso_to_dt(obj.expires_ts),
            obj.tier.value,
            obj.author_or_publisher,
            _iso_to_dt(obj.published_ts),
            _iso_to_dt(obj.ingested_ts),
            obj.url_or_ref,
            obj.raw_ref.split("/")[-1] if obj.raw_ref else "",
            json.dumps(obj.entities),
            json.dumps(obj.linked_events),
            json.dumps(obj.market_keys),
            str(obj.confidence) if obj.confidence is not None else None,
            obj.current_stage,
            obj.parked_reason,
        )
        if on_conflict_do_nothing:
            return row["id"] if row else None
    return obj.id


async def get_object(obj_id: str) -> BrainObject | None:
    """Retrieve a BrainObject by ULID id."""
    pool = await get_pool()
    async with pool.acquire() as conn:
        row = await conn.fetchrow(
            """
            SELECT
                id, kind, source, origin, trust,
                provenance_hash, minio_raw_ref, minio_clean_ref,
                summary, staleness_rule, expires_ts, tier,
                author_or_publisher, published_ts, ingested_ts,
                url_or_ref, entities, linked_events,
                market_keys, confidence,
                current_stage, parked_reason
            FROM brain_objects
            WHERE id = $1
            """,
            obj_id,
        )
    if row is None:
        return None
    return _row_to_object(row)


async def object_exists_by_hash(provenance_hash: str) -> BrainRef | None:
    """Look up an existing BrainRef by provenance hash (dedupe)."""
    pool = await get_pool()
    async with pool.acquire() as conn:
        row = await conn.fetchrow(
            "SELECT id, provenance_hash FROM brain_objects WHERE provenance_hash = $1",
            provenance_hash,
        )
    if row is None:
        return None
    return BrainRef(id=row["id"], provenance_hash=row["provenance_hash"])


async def object_exists_by_raw_sha256(
    raw_sha256: str, source: str | None = None
) -> BrainRef | None:
    """Look up an existing BrainRef by raw content SHA-256 (content-based dedupe)."""
    pool = await get_pool()
    async with pool.acquire() as conn:
        if source is None:
            row = await conn.fetchrow(
                "SELECT id, provenance_hash FROM brain_objects WHERE raw_sha256 = $1",
                raw_sha256,
            )
        else:
            row = await conn.fetchrow(
                "SELECT id, provenance_hash FROM brain_objects "
                "WHERE source = $1 AND raw_sha256 = $2",
                source,
                raw_sha256,
            )
    if row is None:
        return None
    return BrainRef(id=row["id"], provenance_hash=row["provenance_hash"])


async def list_objects(
    tier_exclude: list[str] | None = None,
    tier_filter: list[str] | None = None,
    kind_filter: list[str] | None = None,
) -> list[BrainObject]:
    """List brain_objects with optional filters.

    Args:
        tier_exclude: Exclude objects with these tier values.
        tier_filter: Only include objects with these tier values.
        kind_filter: Only include objects with these kind values.

    Returns:
        List of matching ``BrainObject`` instances, ordered by ``ingested_ts`` DESC.
    """
    pool = await get_pool()
    where_clauses: list[str] = []
    params: list[object] = []
    idx = 1

    if tier_exclude:
        placeholders = ", ".join(f"${i + idx - 1}" for i in range(len(tier_exclude)))
        where_clauses.append(f"tier NOT IN ({placeholders})")
        params.extend(tier_exclude)
        idx += len(tier_exclude)

    if tier_filter:
        placeholders = ", ".join(f"${i + idx - 1}" for i in range(len(tier_filter)))
        where_clauses.append(f"tier IN ({placeholders})")
        params.extend(tier_filter)
        idx += len(tier_filter)

    if kind_filter:
        placeholders = ", ".join(f"${i + idx - 1}" for i in range(len(kind_filter)))
        where_clauses.append(f"kind IN ({placeholders})")
        params.extend(kind_filter)
        idx += len(kind_filter)

    where = ""
    if where_clauses:
        where = "WHERE " + " AND ".join(where_clauses)

    query = f"""
        SELECT
            id, kind, source, origin, trust,
            provenance_hash, minio_raw_ref, minio_clean_ref,
            summary, staleness_rule, expires_ts, tier,
            author_or_publisher, published_ts, ingested_ts,
            url_or_ref, entities, linked_events,
            market_keys, confidence,
            current_stage, parked_reason
        FROM brain_objects
        {where}
        ORDER BY ingested_ts DESC
    """

    async with pool.acquire() as conn:
        rows = await conn.fetch(query, *params)

    return [_row_to_object(row) for row in rows]


async def emit_ingest_event(
    object_id: str,
    source: str,
    ladder_rung: int,
    bytes_count: int,
    status: str = "ok",
) -> None:
    """Insert an ``ingest_events`` row in ClickHouse via the HTTP interface.

    **Best-effort, fire-and-forget** -- ClickHouse is not on the critical path.
    If ClickHouse is unavailable the pipeline continues.  Errors are logged at
    WARN level but never raised.
    """
    from datetime import UTC, datetime  # noqa: PLC0415

    import httpx  # noqa: PLC0415

    ts = datetime.now(UTC).strftime("%Y-%m-%d %H:%M:%S.%f")
    clickhouse_url = os.environ.get("AETHER_CLICKHOUSE__URL", "http://localhost:8123")
    clickhouse_db = os.environ.get("AETHER_CLICKHOUSE__DATABASE", "aether")

    # Escape single quotes for ClickHouse SQL (minimal escaping: '' -> ')
    safe_ts = ts.replace("'", "''")
    safe_source = source.replace("'", "''")
    safe_object_id = object_id.replace("'", "''")
    safe_status = status.replace("'", "''")

    query = (
        f"INSERT INTO {clickhouse_db}.ingest_events "
        "(ts, source, ladder_rung, object_id, bytes, status) "
        "VALUES"
    )
    values = f"('{safe_ts}', '{safe_source}', {ladder_rung}, '{safe_object_id}', {bytes_count}, '{safe_status}')"
    try:
        async with httpx.AsyncClient() as client:
            response = await client.post(
                f"{clickhouse_url}/?query={query}+{values}",
                timeout=5.0,
            )
        if response.status_code != 200:
            logger.warning(
                "ClickHouse ingest_events INSERT returned status %d: %.200s",
                response.status_code,
                response.text,
            )
    except httpx.TimeoutException:
        logger.warning("ClickHouse ingest_events timed out for object %s", object_id)
    except httpx.ConnectError:
        logger.warning(
            "ClickHouse unavailable for ingest_events INSERT (object %s)", object_id
        )
    except Exception as exc:
        logger.warning(
            "ClickHouse ingest_events INSERT failed for object %s: %s", object_id, exc
        )


async def update_object(obj_id: str, **fields: Any) -> None:
    """Update a ``brain_objects`` row with the given fields.

    Builds a dynamic UPDATE statement from the provided keyword arguments.
    Only the specified columns are updated; ``updated_ts`` is always refreshed.
    """
    if not fields:
        return

    pool = await get_pool()
    set_clauses: list[str] = []
    values: list[Any] = []
    idx = 1

    for col, val in fields.items():
        # Map Pythonic field names to SQL column names
        sql_col = _field_to_column(col)
        set_clauses.append(f"{sql_col} = ${idx}")
        values.append(_serialize_value(val))
        idx += 1

    set_clauses.append(f"updated_ts = ${idx}")
    values.append(datetime.now(UTC))
    idx += 1

    values.append(obj_id)

    sql = f"UPDATE brain_objects SET {', '.join(set_clauses)} WHERE id = ${idx}"
    async with pool.acquire() as conn:
        await conn.execute(sql, *values)


# ── Helpers ──────────────────────────────────────────────────────────────


def _row_to_object(row: asyncpg.Record) -> BrainObject:
    return BrainObject(
        id=row["id"],
        kind=ObjectKind(row["kind"]),
        source=row["source"],
        origin=Origin(row["origin"]),
        trust=numeric_to_trust(row.get("trust")),
        author_or_publisher=row.get("author_or_publisher"),
        published_ts=_fmt_ts(row.get("published_ts")),
        ingested_ts=_fmt_ts(row.get("ingested_ts")),
        url_or_ref=row.get("url_or_ref"),
        raw_ref=row.get("minio_raw_ref") or None,
        clean_ref=row.get("minio_clean_ref") or None,
        provenance_hash=row["provenance_hash"],
        summary=row.get("summary"),
        entities=_json_list(row.get("entities", "[]")),
        linked_events=_json_list(row.get("linked_events", "[]")),
        market_keys=_json_list(row.get("market_keys", "[]")),
        confidence=_float_or_none(row.get("confidence")),
        staleness_rule=row.get("staleness_rule"),
        expires_ts=_fmt_ts(row.get("expires_ts")),
        tier=Tier(row.get("tier", "warm")),
        current_stage=row.get("current_stage", "intake"),
        parked_reason=row.get("parked_reason"),
    )


def _json_list(val: Any) -> list[str]:
    if val is None:
        return []
    if isinstance(val, list):
        return [str(v) for v in val]
    if isinstance(val, str):
        return json.loads(val) if val else []
    return []


def _float_or_none(val: Any) -> float | None:
    if val is None:
        return None
    return float(val)


def _fmt_ts(val: Any) -> str | None:
    """Format a timestamp value as ISO 8601 string or return None."""
    if val is None:
        return None
    if isinstance(val, str):
        return val
    # asyncpg returns datetime objects
    if hasattr(val, "isoformat"):
        return str(val)
    return str(val)


# ── Dynamic update helpers ──────────────────────────────────────────────


_FIELD_TO_COLUMN = {
    "clean_ref": "minio_clean_ref",
    "summary": "summary",
    "entities": "entities",
    "linked_events": "linked_events",
    "market_keys": "market_keys",
    "confidence": "confidence",
    "tier": "tier",
    "author_or_publisher": "author_or_publisher",
    "published_ts": "published_ts",
    "url_or_ref": "url_or_ref",
    "source": "source",
    "kind": "kind",
    "trust": "trust",
    "origin": "origin",
    "expires_ts": "expires_ts",
    "staleness_rule": "staleness_rule",
    "current_stage": "current_stage",
    "parked_reason": "parked_reason",
}


def _field_to_column(field: str) -> str:
    """Map Pythonic field name to SQL column name."""
    return _FIELD_TO_COLUMN.get(field, field)


def _serialize_value(val: Any) -> Any:
    """Serialize a Python value for asyncpg parameter binding.

    Handles JSONB fields (list -> json dump), enums, and None.
    """
    if val is None:
        return None
    if isinstance(val, list):
        return json.dumps(val)
    if isinstance(val, dict):
        return json.dumps(val)
    if isinstance(val, str):
        return val
    if isinstance(val, int | float):
        return val
    return str(val)
