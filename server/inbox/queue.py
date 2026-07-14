"""Durable leased notification queue and provider cursor state."""

import json
import os
import sqlite3
from dataclasses import dataclass
from datetime import UTC, datetime, timedelta
from pathlib import Path


@dataclass(frozen=True)
class Notification:
    id: int
    provider: str
    event_key: str
    payload: dict
    attempts: int


def _connect() -> sqlite3.Connection:
    path = Path(os.environ.get("AETHER_INBOX__QUEUE_DB", "data/inbox-queue.sqlite3"))
    path.parent.mkdir(parents=True, exist_ok=True)
    conn = sqlite3.connect(path, timeout=5, isolation_level=None)
    conn.row_factory = sqlite3.Row
    conn.execute("PRAGMA journal_mode=WAL")
    conn.executescript(
        """
        CREATE TABLE IF NOT EXISTS notifications (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            provider TEXT NOT NULL,
            event_key TEXT NOT NULL,
            payload TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'pending',
            attempts INTEGER NOT NULL DEFAULT 0,
            available_ts TEXT NOT NULL,
            lease_until TEXT,
            last_error TEXT,
            created_ts TEXT NOT NULL,
            UNIQUE(provider, event_key)
        );
        CREATE TABLE IF NOT EXISTS provider_cursors (
            provider_key TEXT PRIMARY KEY,
            cursor TEXT NOT NULL,
            updated_ts TEXT NOT NULL
        );
        """
    )
    return conn


def _now() -> datetime:
    return datetime.now(UTC)


def _iso(value: datetime) -> str:
    return value.isoformat()


def enqueue(provider: str, event_key: str, payload: dict) -> bool:
    """Durably enqueue a provider notification before acknowledging it."""
    now = _iso(_now())
    with _connect() as conn:
        cursor = conn.execute(
            """
            INSERT OR IGNORE INTO notifications
                (provider, event_key, payload, available_ts, created_ts)
            VALUES (?, ?, ?, ?, ?)
            """,
            (provider, event_key, json.dumps(payload, separators=(",", ":")), now, now),
        )
        return cursor.rowcount == 1


def claim(lease_seconds: int = 60) -> Notification | None:
    """Atomically lease one pending/retryable notification."""
    now = _now()
    with _connect() as conn:
        conn.execute("BEGIN IMMEDIATE")
        row = conn.execute(
            """
            SELECT * FROM notifications
            WHERE available_ts <= ?
              AND (status = 'pending' OR (status = 'processing' AND lease_until < ?))
            ORDER BY id LIMIT 1
            """,
            (_iso(now), _iso(now)),
        ).fetchone()
        if row is None:
            conn.execute("COMMIT")
            return None
        conn.execute(
            """
            UPDATE notifications SET status = 'processing', attempts = attempts + 1,
                lease_until = ?, last_error = NULL WHERE id = ?
            """,
            (_iso(now + timedelta(seconds=lease_seconds)), row["id"]),
        )
        conn.execute("COMMIT")
    return Notification(
        row["id"],
        row["provider"],
        row["event_key"],
        json.loads(row["payload"]),
        row["attempts"] + 1,
    )


def complete(notification_id: int) -> None:
    with _connect() as conn:
        conn.execute(
            "UPDATE notifications SET status='complete', lease_until=NULL WHERE id=?",
            (notification_id,),
        )


def fail(notification_id: int, error: str, attempts: int) -> None:
    """Release with bounded exponential backoff and a redacted error class."""
    delay = min(300, 2 ** min(attempts, 8))
    with _connect() as conn:
        conn.execute(
            """
            UPDATE notifications SET status='pending', lease_until=NULL,
                available_ts=?, last_error=? WHERE id=?
            """,
            (_iso(_now() + timedelta(seconds=delay)), error[:160], notification_id),
        )


def get_cursor(provider_key: str) -> str | None:
    with _connect() as conn:
        row = conn.execute(
            "SELECT cursor FROM provider_cursors WHERE provider_key=?", (provider_key,)
        ).fetchone()
    return None if row is None else str(row["cursor"])


def set_cursor(provider_key: str, cursor: str) -> None:
    with _connect() as conn:
        conn.execute(
            """
            INSERT INTO provider_cursors(provider_key, cursor, updated_ts) VALUES (?, ?, ?)
            ON CONFLICT(provider_key) DO UPDATE SET cursor=excluded.cursor, updated_ts=excluded.updated_ts
            """,
            (provider_key, cursor, _iso(_now())),
        )


def clear() -> None:
    with _connect() as conn:
        conn.execute("DELETE FROM notifications")
        conn.execute("DELETE FROM provider_cursors")
