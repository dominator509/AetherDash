from unittest.mock import AsyncMock

import pytest

from server.ingest.models import (
    DowngradeDecision,
    FetchBatch,
    FetchedItem,
    LadderRung,
    SourceConfig,
)
from server.ingest.scheduler import FleetScheduler
from server.ingest.state import MemoryStateStore


class Adapter:
    def __init__(self, rung: LadderRung, batches: list[FetchBatch]) -> None:
        self.rung = rung
        self.batches = batches
        self.calls = 0

    async def fetch(self, cursor: str | None, limit: int) -> FetchBatch:
        self.calls += 1
        return self.batches.pop(0)


def config(source: str = "official:test") -> SourceConfig:
    return SourceConfig(
        source=source,
        rung=LadderRung.official_api,
        interval_seconds=60,
    )


def batch(cursor: str = "next") -> FetchBatch:
    return FetchBatch(
        items=(
            FetchedItem(
                kind="news",
                content="clean text",
                raw_content=b"raw text",
                source="official:test",
                trust="high",
            ),
        ),
        next_cursor=cursor,
    )


@pytest.mark.asyncio
async def test_scheduler_records_source_rung_and_advances_cursor_after_store() -> None:
    now = 1_000.0
    state = MemoryStateStore()
    store = AsyncMock()
    scheduler = FleetScheduler(state, store_draft=store, clock=lambda: now)
    await scheduler.register(config(), Adapter(LadderRung.official_api, [batch()]))

    assert await scheduler.poll_due() == 1
    assert await scheduler.process_one() == 1
    assert state.states["official:test"].cursor == "next"
    assert state.states["official:test"].health == "healthy"
    assert store.await_args.kwargs["ladder_rung"] == 1
    assert len(store.await_args.kwargs["trace_id"]) == 26


@pytest.mark.asyncio
async def test_backpressure_prevents_fetch_when_queue_is_full() -> None:
    state = MemoryStateStore()
    first = Adapter(LadderRung.official_api, [batch()])
    second = Adapter(LadderRung.official_api, [batch()])
    scheduler = FleetScheduler(state, queue_size=1, clock=lambda: 1_000.0)
    await scheduler.register(config("a"), first)
    await scheduler.register(config("b"), second)

    assert await scheduler.poll_due() == 1
    assert first.calls == 1
    assert second.calls == 0


@pytest.mark.asyncio
async def test_failed_store_preserves_cursor_and_records_backoff() -> None:
    state = MemoryStateStore()
    store = AsyncMock(side_effect=RuntimeError("store unavailable"))
    scheduler = FleetScheduler(state, store_draft=store, clock=lambda: 1_000.0)
    await scheduler.register(config(), Adapter(LadderRung.official_api, [batch()]))

    await scheduler.poll_due()
    assert await scheduler.process_one() == 0
    saved = state.states["official:test"]
    assert saved.cursor is None
    assert saved.health == "degraded"
    assert saved.consecutive_failures == 1
    assert saved.last_error_code == "RuntimeError"


@pytest.mark.asyncio
async def test_source_is_not_fetched_twice_while_its_batch_is_inflight() -> None:
    state = MemoryStateStore()
    adapter = Adapter(LadderRung.official_api, [batch(), batch("later")])
    scheduler = FleetScheduler(state, store_draft=AsyncMock(), clock=lambda: 1_000.0)
    await scheduler.register(config(), adapter)

    assert await scheduler.poll_due() == 1
    assert await scheduler.poll_due() == 0
    assert adapter.calls == 1
    assert await scheduler.process_one() == 1


@pytest.mark.asyncio
async def test_item_source_mismatch_fails_batch_without_advancing_cursor() -> None:
    state = MemoryStateStore()
    mismatched = FetchBatch(
        items=(
            FetchedItem(
                kind="news",
                content="clean text",
                raw_content=b"raw text",
                source="official:spoofed",
            ),
        ),
        next_cursor="must-not-commit",
    )
    scheduler = FleetScheduler(state, store_draft=AsyncMock(), clock=lambda: 1_000.0)
    await scheduler.register(config(), Adapter(LadderRung.official_api, [mismatched]))

    assert await scheduler.poll_due() == 1
    assert await scheduler.process_one() == 0
    saved = state.states["official:test"]
    assert saved.cursor is None
    assert saved.last_error_code == "ValueError"


@pytest.mark.asyncio
async def test_adapter_and_declared_rung_must_match() -> None:
    scheduler = FleetScheduler(MemoryStateStore())
    adapter = Adapter(LadderRung.rss_or_sitemap, [batch()])
    with pytest.raises(ValueError, match="downgrade decision"):
        await scheduler.register(config(), adapter)


@pytest.mark.asyncio
async def test_explicit_downgrade_is_recorded_and_actual_rung_is_used() -> None:
    state = MemoryStateStore()
    store = AsyncMock()
    scheduler = FleetScheduler(state, store_draft=store, clock=lambda: 1_000.0)
    adapter = Adapter(LadderRung.rss_or_sitemap, [batch()])
    decision = DowngradeDecision(
        source="official:test",
        from_rung=LadderRung.official_api,
        to_rung=LadderRung.rss_or_sitemap,
        reason="official endpoint is temporarily unavailable",
        approved_by="operator:test",
    )

    await scheduler.register(config(), adapter, downgrade=decision)
    await scheduler.poll_due()
    assert await scheduler.process_one() == 1
    assert state.downgrades == [decision]
    assert store.await_args.kwargs["ladder_rung"] == 3


def test_bot_bypass_requirement_is_refused_at_config_time() -> None:
    with pytest.raises(ValueError, match="anti-bot circumvention"):
        SourceConfig(
            source="crawl:blocked",
            rung=LadderRung.robots_compliant_crawl,
            interval_seconds=60,
            requires_bot_bypass=True,
        )
