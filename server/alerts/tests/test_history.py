"""Tests for the alert history module (Postgres persistence, migration 0024).

Uses mocked ``asyncpg`` pools to avoid requiring a running Postgres.
"""

from unittest.mock import AsyncMock, Mock, patch

import pytest
import pytest_asyncio

from server.alerts.history import get_alert, get_alerts, record_alert
from server.alerts.models import AlertPayload

# ── Fixtures ─────────────────────────────────────────────────────────────


@pytest_asyncio.fixture
async def mock_history() -> AsyncMock:
    """Patch ``server.alerts.history.get_pool`` with a mock pool.

    Uses ``Mock`` (not ``AsyncMock``) for ``pool.acquire`` because
    asyncpg's ``Pool.acquire()`` is a synchronous call that returns an
    async context manager -- not a coroutine.

    Returns
    -------
    AsyncMock
        The mock connection -- use it to set ``fetchrow``, ``fetch``,
        and ``execute`` return values per test.
    """
    mock_conn = AsyncMock()
    # async context manager returned by pool.acquire()
    mock_cm = Mock()
    mock_cm.__aenter__ = AsyncMock(return_value=mock_conn)
    mock_cm.__aexit__ = AsyncMock(return_value=None)

    pool = Mock()
    pool.acquire.return_value = mock_cm

    with patch("server.alerts.history.get_pool", return_value=pool):
        yield mock_conn


def _sample_payload(**overrides: str | float | list[str]) -> AlertPayload:
    """Helper to build an ``AlertPayload`` with sensible defaults."""
    fields: dict = {
        "alert_id": "01ARZ3NDEKTSV4RRFFQ69G5FAV",
        "rule_name": "test_rule",
        "opportunity_id": "OPP-001",
        "channel": "telegram",
        "summary": "Alert: test_rule -- OPP-001 (net_edge=0.05, confidence=0.85)",
        "net_edge": "0.05",
        "confidence": 0.85,
        "action": "simulate",
        "inline_actions": ["simulate", "ignore"],
    }
    fields.update(overrides)
    return AlertPayload(**fields)


def _mock_row(**overrides: object) -> dict:
    """Simulate an asyncpg Record (plain dict is sufficient)."""
    row: dict = {
        "id": "01ARZ3NDEKTSV4RRFFQ69G5FAV",
        "rule_name": "test_rule",
        "opportunity_id": "OPP-001",
        "channel": "telegram",
        "summary": "Alert: test_rule -- OPP-001 (net_edge=0.05, confidence=0.85)",
        "net_edge": "0.05",
        "confidence": 0.85,
        "action": "simulate",
        "operator_id": None,
        "status": "sent",
        "created_ts": "2026-07-12T10:00:00Z",
    }
    row.update(overrides)
    return row


# ── record_alert tests ──────────────────────────────────────────────────


class TestRecordAlert:
    """Tests for the ``record_alert`` function."""

    @pytest.mark.asyncio
    async def test_record_alert_inserts_row(self, mock_history: AsyncMock) -> None:
        """record_alert calls execute with correct SQL."""
        payload = _sample_payload()
        await record_alert(payload)
        mock_history.execute.assert_called_once()
        call_args = mock_history.execute.call_args[0]
        assert "INSERT INTO alert_history" in call_args[0]
        assert call_args[1] == payload.alert_id
        assert call_args[2] == payload.rule_name

    @pytest.mark.asyncio
    async def test_record_alert_idempotent(self, mock_history: AsyncMock) -> None:
        """record_alert atomically suppresses sent duplicates but reclaims failures."""
        payload = _sample_payload()
        await record_alert(payload)
        sql = mock_history.execute.call_args[0][0]
        assert "ON CONFLICT" in sql
        assert "DO UPDATE" in sql
        assert "alert_history.status = 'failed'" in sql


# ── get_alert tests ─────────────────────────────────────────────────────


class TestGetAlert:
    """Tests for the ``get_alert`` function."""

    @pytest.mark.asyncio
    async def test_get_alert_returns_payload(self, mock_history: AsyncMock) -> None:
        """get_alert returns an AlertPayload for a found row."""
        mock_history.fetchrow.return_value = _mock_row()
        result = await get_alert("01ARZ3NDEKTSV4RRFFQ69G5FAV")
        assert result is not None
        assert isinstance(result, AlertPayload)
        assert result.alert_id == "01ARZ3NDEKTSV4RRFFQ69G5FAV"
        assert result.rule_name == "test_rule"
        assert result.action == "simulate"

    @pytest.mark.asyncio
    async def test_get_nonexistent_alert_returns_none(
        self, mock_history: AsyncMock
    ) -> None:
        """get_alert returns None for a missing ID."""
        mock_history.fetchrow.return_value = None
        result = await get_alert("NONEXISTENT")
        assert result is None


# ── get_alerts tests ────────────────────────────────────────────────────


class TestGetAlerts:
    """Tests for the ``get_alerts`` function."""

    @pytest.mark.asyncio
    async def test_get_alerts_returns_list(self, mock_history: AsyncMock) -> None:
        """get_alerts returns a list of AlertPayload."""
        mock_history.fetch.return_value = [
            _mock_row(id="AAA"),
            _mock_row(id="BBB"),
        ]
        results = await get_alerts()
        assert len(results) == 2
        assert all(isinstance(r, AlertPayload) for r in results)
        assert results[0].alert_id == "AAA"
        assert results[1].alert_id == "BBB"

    @pytest.mark.asyncio
    async def test_get_alerts_empty(self, mock_history: AsyncMock) -> None:
        """get_alerts returns empty list when no rows."""
        mock_history.fetch.return_value = []
        results = await get_alerts()
        assert results == []

    @pytest.mark.asyncio
    async def test_get_alerts_filtered_by_channel(
        self, mock_history: AsyncMock
    ) -> None:
        """Channel filter is included in the SQL query."""
        mock_history.fetch.return_value = [_mock_row(channel="telegram")]
        await get_alerts(channel="telegram")
        sql = mock_history.fetch.call_args[0][0]
        assert "WHERE" in sql
        assert "channel = $1" in sql

    @pytest.mark.asyncio
    async def test_get_alerts_with_since(self, mock_history: AsyncMock) -> None:
        """Since filter is included in the SQL query."""
        mock_history.fetch.return_value = [_mock_row()]
        await get_alerts(since="2026-01-01T00:00:00Z")
        sql = mock_history.fetch.call_args[0][0]
        assert "WHERE" in sql
        assert "created_ts >= $1" in sql

    @pytest.mark.asyncio
    async def test_get_alerts_limited(self, mock_history: AsyncMock) -> None:
        """Limit is respected in the SQL query."""
        mock_history.fetch.return_value = [_mock_row()] * 10
        results = await get_alerts(limit=10)
        assert len(results) == 10
        sql = mock_history.fetch.call_args[0][0]
        assert "LIMIT $1" in sql

    @pytest.mark.asyncio
    async def test_get_alerts_default_limit(self, mock_history: AsyncMock) -> None:
        """Default limit is 50 when not specified."""
        mock_history.fetch.return_value = [_mock_row()] * 50
        results = await get_alerts()
        assert len(results) == 50
        sql = mock_history.fetch.call_args[0][0]
        # LIMIT with default 50 -- params: only $1 (the limit)
        assert "LIMIT $1" in sql
