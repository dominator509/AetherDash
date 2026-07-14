"""Durable, atomic content-hash deduplication for provider retries."""

import os
import sqlite3
from pathlib import Path


def _connect() -> sqlite3.Connection:
    path = Path(os.environ.get("AETHER_INBOX__DEDUP_DB", "data/inbox-dedup.sqlite3"))
    path.parent.mkdir(parents=True, exist_ok=True)
    conn = sqlite3.connect(path, timeout=5)
    conn.execute("PRAGMA journal_mode=WAL")
    conn.execute(
        "CREATE TABLE IF NOT EXISTS seen_content (hash TEXT PRIMARY KEY, seen_ts TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP)"
    )
    return conn


def is_duplicate(raw_hash: str) -> bool:
    """Return whether a hash has already been atomically claimed."""
    with _connect() as conn:
        return (
            conn.execute(
                "SELECT 1 FROM seen_content WHERE hash = ?", (raw_hash,)
            ).fetchone()
            is not None
        )


def mark_seen(raw_hash: str) -> bool:
    """Atomically claim a hash; return False when another worker owns it."""
    with _connect() as conn:
        cursor = conn.execute(
            """
            INSERT INTO seen_content(hash) VALUES (?)
            ON CONFLICT(hash) DO UPDATE SET seen_ts = CURRENT_TIMESTAMP
            WHERE seen_content.seen_ts < datetime('now', '-5 minutes')
            """,
            (raw_hash,),
        )
        return cursor.rowcount == 1


def clear() -> None:
    """Clear hashes for isolated tests."""
    with _connect() as conn:
        conn.execute("DELETE FROM seen_content")
