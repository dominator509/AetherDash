import os

import asyncpg
import pytest

from server.ingest.models import DowngradeDecision, LadderRung, SourceConfig
from server.ingest.scoring.reliability import PostgresReliabilityEvidence
from server.ingest.sources.manual import PostgresManualReviewQueue
from server.ingest.state import PostgresStateStore

pytestmark = [
    pytest.mark.integration,
    pytest.mark.asyncio,
    pytest.mark.skipif(
        os.environ.get("AETHER_INTEGRATION_TEST") != "1",
        reason="set AETHER_INTEGRATION_TEST=1 for live Postgres tests",
    ),
]


async def test_compliance_decision_and_manual_review_are_durable() -> None:
    pool = await asyncpg.create_pool(os.environ["DATABASE_URL"], min_size=1, max_size=2)
    source = "ep206:manual:integration"
    state = PostgresStateStore(pool)
    queue = PostgresManualReviewQueue(pool)
    try:
        await state.register(
            [
                SourceConfig(
                    source=source,
                    rung=LadderRung.rss_or_sitemap,
                    interval_seconds=60,
                )
            ]
        )
        decision = DowngradeDecision(
            source=source,
            from_rung=LadderRung.official_api,
            to_rung=LadderRung.rss_or_sitemap,
            reason="official API is unavailable during integration proof",
            approved_by="operator:integration",
        )
        await state.record_downgrade(decision)
        decision_row = await pool.fetchrow(
            """
            SELECT from_rung,to_rung,reason,approved_by
            FROM ingest_rung_decisions WHERE source=$1
            """,
            source,
        )
        assert decision_row is not None
        assert decision_row["from_rung"] == 1
        assert decision_row["to_rung"] == 3
        assert decision_row["approved_by"] == "operator:integration"

        approved_id = await queue.submit(
            source=source,
            kind="note",
            content="approved operator evidence",
            raw_content=b"approved operator evidence",
            submitted_by="operator:submitter",
        )
        rejected_id = await queue.submit(
            source=source,
            kind="note",
            content="rejected operator evidence",
            raw_content=b"rejected operator evidence",
            submitted_by="operator:submitter",
        )
        assert await queue.review(
            approved_id, approved=True, reviewed_by="operator:reviewer"
        )
        assert await queue.review(
            rejected_id, approved=False, reviewed_by="operator:reviewer"
        )
        rows = await queue.approved_after(source, None, 10)
        assert [item_id for item_id, _ in rows] == [approved_id]
        assert rows[0][1].content == "approved operator evidence"
    finally:
        await pool.execute(
            "DELETE FROM ingest_manual_review_queue WHERE source=$1", source
        )
        await pool.execute("DELETE FROM ingest_rung_decisions WHERE source=$1", source)
        await pool.execute("DELETE FROM ingest_source_state WHERE source=$1", source)
        await pool.close()


async def test_correlation_and_operator_feedback_are_aggregated() -> None:
    pool = await asyncpg.create_pool(os.environ["DATABASE_URL"], min_size=1, max_size=2)
    connection = await pool.acquire()
    transaction = connection.transaction()
    await transaction.start()
    source = "ep206:reliability:integration"
    object_id = "01ARZ3NDEKTSV4RRFFQ69G5FQ0"
    try:
        await connection.execute(
            """
            INSERT INTO brain_objects
                (id,kind,source,origin,trust,provenance_hash,raw_sha256)
            VALUES ($1,'news',$2,'ingest_fleet',0.9,$3,$4)
            """,
            object_id,
            source,
            "1" * 64,
            "2" * 64,
        )
        repository = PostgresReliabilityEvidence(connection)
        await repository.record_correlation(
            source=source, object_id=object_id, correlated=True
        )
        await repository.record_feedback(
            source=source,
            actor_id="operator:integration",
            positive=False,
            reason="fixture correction",
        )
        summary = await repository.summary(source)
        assert summary.correlated_moves == 1
        assert summary.uncorrelated_moves == 0
        assert summary.positive_feedback == 0
        assert summary.negative_feedback == 1
    finally:
        await transaction.rollback()
        await pool.release(connection)
        await pool.close()
