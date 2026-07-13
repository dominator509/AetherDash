"""Unit tests for Brain maintenance jobs — tiering + staleness (Milestone 6).

Uses mocks for the Postgres store layer so tests run without infrastructure.
"""

from datetime import UTC, datetime, timedelta
from unittest.mock import AsyncMock, patch

import pytest
import ulid

from server.brain.jobs.staleness import run_staleness_job
from server.brain.jobs.tiering import (
    _market_keys_are_all_resolved,
    _parse_resolved_markets,
    run_tiering_job,
)
from server.brain.models import (
    BrainObject,
    ObjectKind,
    Origin,
    Tier,
    TrustLevel,
    now_iso,
)


def _make_brain_object(**overrides: object) -> BrainObject:
    """Build a BrainObject with sensible defaults."""
    defaults: dict = {
        "id": "01ARZ3NDEKTSV4RRFFQ69G5FAV",
        "kind": ObjectKind.news,
        "source": "feed://news-api",
        "origin": Origin.ingest_fleet,
        "trust": TrustLevel.medium,
        "ingested_ts": now_iso(),
        "provenance_hash": "a" * 64,
        "tier": Tier.hot,
        "entities": [],
        "linked_events": [],
        "market_keys": [],
        "summary": "Test summary for job testing.",
    }
    defaults.update(overrides)
    return BrainObject(**defaults)


# ═══════════════════════════════════════════════════════════════════════════
# Tiering tests
# ═══════════════════════════════════════════════════════════════════════════


@pytest.mark.asyncio
@patch("server.brain.store.list_objects")
@patch("server.brain.store.update_object")
async def test_tiering_hot_objects_stay_hot(
    mock_update: AsyncMock,
    mock_list: AsyncMock,
) -> None:
    """Recently ingested objects with market_keys stay hot."""
    obj_id = str(ulid.new())
    obj = _make_brain_object(
        id=obj_id,
        tier=Tier.hot,
        market_keys=["SPY-options"],
        ingested_ts=now_iso(),  # just now
    )
    mock_list.return_value = [obj]

    await run_tiering_job()

    # update_object should NOT be called for this object (it's already hot)
    for call_args in mock_update.call_args_list:
        args, kwargs = call_args
        oid = args[0] if args else kwargs.get("obj_id", "")
        if oid == obj_id:
            assert kwargs.get("tier") is None or kwargs.get("tier") == "hot"


@pytest.mark.asyncio
@patch("server.brain.store.list_objects")
@patch("server.brain.store.update_object")
async def test_tiering_old_objects_go_cold(
    mock_update: AsyncMock,
    mock_list: AsyncMock,
) -> None:
    """Objects ingested more than 90 days ago with no market_keys go cold."""
    old_ts = (datetime.now(UTC) - timedelta(days=100)).strftime(
        "%Y-%m-%dT%H:%M:%S.000Z"
    )
    obj_id = str(ulid.new())
    obj = _make_brain_object(
        id=obj_id,
        tier=Tier.warm,
        market_keys=[],
        ingested_ts=old_ts,
    )
    mock_list.return_value = [obj]

    moves = await run_tiering_job()

    assert moves.get("to_cold", 0) >= 1
    # update_object should have been called with tier=cold
    cold_update_calls = [
        call
        for call in mock_update.call_args_list
        if call.args[0] == obj_id
        and (isinstance(call.kwargs.get("tier"), str) and call.kwargs["tier"] == "cold")
    ]
    assert len(cold_update_calls) >= 1


@pytest.mark.asyncio
@patch("server.brain.store.list_objects")
@patch("server.brain.store.update_object")
async def test_tiering_warm_objects_stay_warm(
    mock_update: AsyncMock,
    mock_list: AsyncMock,
) -> None:
    """Objects ingested 30 days ago (less than 90) stay warm when no hot criteria."""
    mid_ts = (datetime.now(UTC) - timedelta(days=30)).strftime("%Y-%m-%dT%H:%M:%S.000Z")
    obj_id = str(ulid.new())
    obj = _make_brain_object(
        id=obj_id,
        tier=Tier.warm,
        market_keys=[],
        ingested_ts=mid_ts,
    )
    mock_list.return_value = [obj]

    await run_tiering_job()

    warm_update_calls_explicit = [
        call for call in mock_update.call_args_list if call.args[0] == obj_id
    ]
    # Should not have been updated to different tier
    if warm_update_calls_explicit:
        for call in warm_update_calls_explicit:
            assert call.kwargs.get("tier") in (None, "warm")


@pytest.mark.asyncio
@patch("server.brain.jobs.tiering._drop_qdrant_vectors")
@patch("server.brain.store.list_objects")
@patch("server.brain.store.update_object")
async def test_tiering_cold_objects_drop_qdrant_vectors(
    mock_update: AsyncMock,
    mock_list: AsyncMock,
    mock_drop: AsyncMock,
) -> None:
    """Cold-tier objects have their Qdrant vectors dropped."""
    old_ts = (datetime.now(UTC) - timedelta(days=100)).strftime(
        "%Y-%m-%dT%H:%M:%S.000Z"
    )
    obj_id = str(ulid.new())
    obj = _make_brain_object(
        id=obj_id,
        tier=Tier.warm,
        market_keys=[],
        ingested_ts=old_ts,
    )
    mock_list.return_value = [obj]

    moves = await run_tiering_job()
    assert moves.get("to_cold", 0) >= 1
    mock_drop.assert_awaited_once_with(obj_id)


# ═══════════════════════════════════════════════════════════════════════════
# Resolved-market sweep tests
# ═══════════════════════════════════════════════════════════════════════════


def test_parse_resolved_markets_empty() -> None:
    """Empty env var produces empty resolved set."""
    with patch("server.brain.jobs.tiering._AETHER_RESOLVED_MARKETS", ""):
        assert _parse_resolved_markets() == set()


def test_parse_resolved_markets_populated() -> None:
    """Comma-separated env var is parsed correctly."""
    with patch(
        "server.brain.jobs.tiering._AETHER_RESOLVED_MARKETS",
        "POLYMARKET:123,SPY-240712",
    ):
        result = _parse_resolved_markets()
        assert result == {"POLYMARKET:123", "SPY-240712"}


def test_market_keys_are_all_resolved_positive() -> None:
    """All market keys in resolved set -> True."""
    resolved = {"POLYMARKET:123", "SPY-240712"}
    assert _market_keys_are_all_resolved(["POLYMARKET:123"], resolved) is True
    assert (
        _market_keys_are_all_resolved(["SPY-240712", "POLYMARKET:123"], resolved)
        is True
    )


def test_market_keys_are_all_resolved_negative() -> None:
    """Some market keys not in resolved set -> False."""
    resolved = {"POLYMARKET:123"}
    assert _market_keys_are_all_resolved(["POLYMARKET:456"], resolved) is False
    assert (
        _market_keys_are_all_resolved(["POLYMARKET:123", "UNKNOWN"], resolved) is False
    )


def test_market_keys_are_all_resolved_empty_keys() -> None:
    """Empty market keys list -> False."""
    assert _market_keys_are_all_resolved([], {"POLYMARKET:123"}) is False


@pytest.mark.asyncio
@patch("server.brain.jobs.tiering._AETHER_RESOLVED_MARKETS", "RESOLVED-1")
@patch("server.brain.jobs.tiering._drop_qdrant_vectors")
@patch("server.brain.store.list_objects")
@patch("server.brain.store.update_object")
@patch("server.brain.store.insert_object")
async def test_resolved_market_rollup_creates_synthesis(
    mock_insert: AsyncMock,
    mock_update: AsyncMock,
    mock_list: AsyncMock,
    mock_drop: AsyncMock,
) -> None:
    """Resolved market objects get roll-up synthesis and go cold."""
    obj_id = str(ulid.new())
    obj = _make_brain_object(
        id=obj_id,
        tier=Tier.warm,
        market_keys=["RESOLVED-1"],
        summary="Content for resolved market.",
        ingested_ts=(datetime.now(UTC) - timedelta(days=1)).strftime(
            "%Y-%m-%dT%H:%M:%S.000Z"
        ),
    )
    mock_list.return_value = [obj]

    moves = await run_tiering_job()
    assert moves.get("rollup_created", 0) >= 1
    assert moves.get("to_cold", 0) >= 1
    mock_insert.assert_awaited_once()
    # Update should set the original to cold
    cold_update_calls = [
        c
        for c in mock_update.call_args_list
        if c.args[0] == obj_id and c.kwargs.get("tier") == "cold"
    ]
    assert len(cold_update_calls) >= 1


@pytest.mark.asyncio
@patch("server.brain.store.list_objects")
@patch("server.brain.store.update_object")
async def test_tiering_no_resolved_markets_skips_rollup(
    mock_update: AsyncMock,
    mock_list: AsyncMock,
) -> None:
    """When no resolved markets configured, rollup is skipped."""
    recent_ts = (datetime.now(UTC) - timedelta(hours=1)).strftime(
        "%Y-%m-%dT%H:%M:%S.000Z"
    )
    obj = _make_brain_object(
        id=str(ulid.new()),
        tier=Tier.hot,
        market_keys=["OPEN-MARKET"],
        ingested_ts=recent_ts,
    )
    mock_list.return_value = [obj]

    # When _AETHER_RESOLVED_MARKETS is empty, _run_resolved_market_rollup
    # returns {} immediately. The tiering job should complete without error.
    with patch("server.brain.jobs.tiering._AETHER_RESOLVED_MARKETS", ""):
        moves = await run_tiering_job()

    # No objects should be re-tiered (hot stays hot)
    assert moves.get("to_cold", 0) == 0
    assert moves.get("to_warm", 0) == 0
    assert moves.get("to_hot", 0) == 0
    assert moves.get("rollup_created", 0) == 0


# ═══════════════════════════════════════════════════════════════════════════
# Staleness tests
# ═══════════════════════════════════════════════════════════════════════════


@pytest.mark.asyncio
@patch("server.brain.store.list_objects")
@patch("server.brain.store.update_object")
async def test_staleness_news_goes_stale_after_72h(
    mock_update: AsyncMock,
    mock_list: AsyncMock,
) -> None:
    """News objects older than 72h should be marked stale."""
    old_ts = (datetime.now(UTC) - timedelta(hours=100)).strftime(
        "%Y-%m-%dT%H:%M:%S.000Z"
    )
    obj = _make_brain_object(
        id=str(ulid.new()),
        kind=ObjectKind.news,
        staleness_rule=None,
        ingested_ts=old_ts,
    )
    mock_list.return_value = [obj]

    result = await run_staleness_job()
    assert result["stale"] >= 1


@pytest.mark.asyncio
@patch("server.brain.store.list_objects")
@patch("server.brain.store.update_object")
async def test_staleness_email_never_auto_stales(
    mock_update: AsyncMock,
    mock_list: AsyncMock,
) -> None:
    """Email objects should never be auto-staled regardless of age."""
    old_ts = (datetime.now(UTC) - timedelta(days=365)).strftime(
        "%Y-%m-%dT%H:%M:%S.000Z"
    )
    obj = _make_brain_object(
        id=str(ulid.new()),
        kind=ObjectKind.email,
        staleness_rule=None,
        ingested_ts=old_ts,
    )
    mock_list.return_value = [obj]

    result = await run_staleness_job()
    assert result["stale"] == 0


@pytest.mark.asyncio
@patch("server.brain.store.list_objects")
@patch("server.brain.store.update_object")
async def test_staleness_note_never_auto_stales(
    mock_update: AsyncMock,
    mock_list: AsyncMock,
) -> None:
    """Note objects should never be auto-staled regardless of age."""
    old_ts = (datetime.now(UTC) - timedelta(days=365)).strftime(
        "%Y-%m-%dT%H:%M:%S.000Z"
    )
    obj = _make_brain_object(
        id=str(ulid.new()),
        kind=ObjectKind.note,
        staleness_rule=None,
        ingested_ts=old_ts,
    )
    mock_list.return_value = [obj]

    result = await run_staleness_job()
    assert result["stale"] == 0


@pytest.mark.asyncio
@patch("server.brain.store.list_objects")
@patch("server.brain.store.update_object")
async def test_staleness_report_goes_stale_after_30d(
    mock_update: AsyncMock,
    mock_list: AsyncMock,
) -> None:
    """Report objects older than 30d should be marked stale."""
    old_ts = (datetime.now(UTC) - timedelta(days=60)).strftime("%Y-%m-%dT%H:%M:%S.000Z")
    obj = _make_brain_object(
        id=str(ulid.new()),
        kind=ObjectKind.report,
        staleness_rule=None,
        ingested_ts=old_ts,
    )
    mock_list.return_value = [obj]

    result = await run_staleness_job()
    assert result["stale"] >= 1


@pytest.mark.asyncio
@patch("server.brain.store.list_objects")
@patch("server.brain.store.update_object")
async def test_staleness_recent_news_not_stale(
    mock_update: AsyncMock,
    mock_list: AsyncMock,
) -> None:
    """Recent news objects (within 72h) should not be marked stale."""
    recent_ts = (datetime.now(UTC) - timedelta(hours=12)).strftime(
        "%Y-%m-%dT%H:%M:%S.000Z"
    )
    obj = _make_brain_object(
        id=str(ulid.new()),
        kind=ObjectKind.news,
        staleness_rule=None,
        ingested_ts=recent_ts,
    )
    mock_list.return_value = [obj]

    result = await run_staleness_job()
    assert result["stale"] == 0
