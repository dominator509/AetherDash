"""Alert history -- persistent storage for dispatched alerts.

Uses the ``alert_history`` table (migration 0024) and follows the same
``asyncpg`` pool pattern as ``server.brain.store``.
"""

import logging
import os

import asyncpg

from server.alerts.models import AlertPayload

logger = logging.getLogger(__name__)

_DATABASE_URL = os.environ.get(
    "DATABASE_URL", "postgres://aether:aether@localhost:5432/aether"
)

_pool: asyncpg.Pool | None = None


async def get_pool() -> asyncpg.Pool:
    """Get or create the database connection pool."""
    global _pool  # noqa: PLW0603
    if _pool is None:
        _pool = await asyncpg.create_pool(
            _DATABASE_URL,
            min_size=1,
            max_size=5,
        )
    return _pool


async def close_pool() -> None:
    """Close the database connection pool."""
    global _pool  # noqa: PLW0603
    if _pool is not None:
        await _pool.close()
        _pool = None


async def record_alert(alert: AlertPayload) -> bool:
    """Store an alert in the ``alert_history`` table.

    Idempotent: ``ON CONFLICT (id) DO NOTHING`` prevents duplicate entries.

    Args:
        alert: The alert payload to persist.
    """
    pool = await get_pool()
    async with pool.acquire() as conn:
        result = await conn.execute(
            """
            INSERT INTO alert_history (
                id, rule_name, opportunity_id, channel,
                summary, net_edge, confidence, action, status
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'pending')
            ON CONFLICT (opportunity_id, rule_name, channel) DO UPDATE
            SET id = EXCLUDED.id, status = 'pending', message_id = NULL,
                last_error = NULL, updated_ts = now()
            WHERE alert_history.status = 'failed'
               OR (alert_history.status = 'pending'
                   AND alert_history.updated_ts < now() - interval '5 minutes')
            """,
            alert.alert_id,
            alert.rule_name,
            alert.opportunity_id,
            alert.channel,
            alert.summary,
            alert.net_edge,
            alert.confidence,
            alert.action,
        )
    return result == "INSERT 0 1"


async def update_delivery(
    alert_id: str,
    *,
    status: str,
    message_id: str | None,
    last_error: str | None,
) -> None:
    """Record a channel attempt and its observable result."""
    pool = await get_pool()
    async with pool.acquire() as conn:
        await conn.execute(
            """
            UPDATE alert_history
            SET status = $2, message_id = $3, last_error = $4,
                attempts = attempts + 1, updated_ts = now()
            WHERE id = $1
            """,
            alert_id,
            status,
            message_id,
            last_error,
        )


async def get_alerts(
    since: str | None = None,
    channel: str | None = None,
    limit: int = 50,
) -> list[AlertPayload]:
    """Query alert history with optional filters.

    Parameters
    ----------
    since:
        ISO 8601 timestamp -- only return alerts created at or after
        this time.
    channel:
        Only return alerts for this channel (e.g. ``"telegram"``).
    limit:
        Maximum number of alerts to return (default 50).

    Returns
    -------
    list[AlertPayload]
        Alerts ordered by ``created_ts`` DESC.
    """
    pool = await get_pool()
    conditions: list[str] = []
    params: list[object] = []
    idx = 1

    if since is not None:
        conditions.append(f"created_ts >= ${idx}::timestamptz")
        params.append(since)
        idx += 1

    if channel is not None:
        conditions.append(f"channel = ${idx}")
        params.append(channel)
        idx += 1

    where_clause = ""
    if conditions:
        where_clause = "WHERE " + " AND ".join(conditions)

    query = f"""
        SELECT
            id, rule_name, opportunity_id, channel,
            summary, net_edge, confidence, action,
            operator_id, status, message_id, attempts, last_error, created_ts
        FROM alert_history
        {where_clause}
        ORDER BY created_ts DESC
        LIMIT ${idx}
    """
    params.append(limit)

    async with pool.acquire() as conn:
        rows = await conn.fetch(query, *params)

    return [_row_to_payload(row) for row in rows]


async def get_alert(alert_id: str) -> AlertPayload | None:
    """Get a single alert by its ULID.

    Args:
        alert_id: The ULID of the alert to retrieve.

    Returns:
        The ``AlertPayload`` if found, ``None`` otherwise.
    """
    pool = await get_pool()
    async with pool.acquire() as conn:
        row = await conn.fetchrow(
            """
            SELECT
                id, rule_name, opportunity_id, channel,
                summary, net_edge, confidence, action,
                operator_id, status, message_id, attempts, last_error, created_ts
            FROM alert_history
            WHERE id = $1
            """,
            alert_id,
        )
    if row is None:
        return None
    return _row_to_payload(row)


def _row_to_payload(row: asyncpg.Record) -> AlertPayload:
    """Convert an ``alert_history`` row to an ``AlertPayload``.

    Fields not stored in the table (``inline_actions``) are reconstructed
    with sensible defaults.
    """
    return AlertPayload(
        alert_id=row["id"],
        rule_name=row["rule_name"],
        opportunity_id=row["opportunity_id"],
        channel=row["channel"],
        summary=row["summary"],
        net_edge=str(row["net_edge"]) if row["net_edge"] is not None else "0",
        confidence=(float(row["confidence"]) if row["confidence"] is not None else 0.0),
        action=row["action"],
        inline_actions=["simulate", "execute", "ignore"],
        message_id=row.get("message_id"),
        status=row["status"],
        attempts=row.get("attempts", 0),
        last_error=row.get("last_error"),
    )
