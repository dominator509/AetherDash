"""Rung 6 durable operator-reviewed ingestion queue."""

from typing import Protocol

import asyncpg
import ulid

from server.ingest.models import FetchBatch, FetchedItem, LadderRung


class ManualReviewRepository(Protocol):
    async def approved_after(
        self, source: str, cursor: str | None, limit: int
    ) -> tuple[tuple[str, FetchedItem], ...]: ...


class PostgresManualReviewQueue:
    def __init__(self, pool: asyncpg.Pool) -> None:
        self.pool = pool

    async def submit(
        self,
        *,
        source: str,
        kind: str,
        content: str,
        raw_content: bytes,
        submitted_by: str,
    ) -> str:
        item_id = str(ulid.new())
        await self.pool.execute(
            """
            INSERT INTO ingest_manual_review_queue
                (id,source,kind,content,raw_content,submitted_by)
            VALUES ($1,$2,$3,$4,$5,$6)
            """,
            item_id,
            source,
            kind,
            content,
            raw_content,
            submitted_by,
        )
        return item_id

    async def review(self, item_id: str, *, approved: bool, reviewed_by: str) -> bool:
        result = await self.pool.execute(
            """
            UPDATE ingest_manual_review_queue
            SET status=$2,reviewed_by=$3,reviewed_ts=now()
            WHERE id=$1 AND status='pending'
            """,
            item_id,
            "approved" if approved else "rejected",
            reviewed_by,
        )
        return result == "UPDATE 1"

    async def approved_after(
        self, source: str, cursor: str | None, limit: int
    ) -> tuple[tuple[str, FetchedItem], ...]:
        rows = await self.pool.fetch(
            """
            SELECT id,kind,content,raw_content FROM ingest_manual_review_queue
            WHERE source=$1 AND status='approved' AND ($2::text IS NULL OR id>$2)
            ORDER BY id LIMIT $3
            """,
            source,
            cursor,
            limit,
        )
        return tuple(
            (
                row["id"],
                FetchedItem(
                    kind=row["kind"],
                    content=row["content"],
                    raw_content=bytes(row["raw_content"]),
                    source=source,
                ),
            )
            for row in rows
        )


class ManualReviewAdapter:
    rung = LadderRung.manual_review

    def __init__(self, *, source: str, repository: ManualReviewRepository) -> None:
        self.source = source
        self.repository = repository

    async def fetch(self, cursor: str | None, limit: int) -> FetchBatch:
        rows = await self.repository.approved_after(self.source, cursor, limit)
        return FetchBatch(
            items=tuple(item for _, item in rows),
            next_cursor=rows[-1][0] if rows else cursor,
        )
