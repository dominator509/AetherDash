"""Unit tests for Brain explain tree assembly (Milestone 5).

Uses mocks for the Postgres store layer so tests run without infrastructure.
"""

from unittest.mock import AsyncMock, patch

import pytest

from server.brain.explain import explain
from server.brain.models import (
    BrainObject,
    ObjectKind,
    Origin,
    Tier,
    TrustLevel,
    now_iso,
)


def _make_brain_object(**overrides: object) -> BrainObject:
    """Build a BrainObject with sensible defaults for testing."""
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
        "summary": "Federal Reserve raises interest rates by 25bps.",
    }
    defaults.update(overrides)
    return BrainObject(**defaults)


# ── Tests ─────────────────────────────────────────────────────────────────


@pytest.mark.asyncio
@patch("server.brain.store.get_object")
async def test_explain_returns_tree_for_valid_opportunity(mock_get: AsyncMock) -> None:
    """Explain returns a tree dict for a valid opportunity ID."""
    obj = _make_brain_object()
    mock_get.return_value = obj

    result = await explain("01ARZ3NDEKTSV4RRFFQ69G5FAV")
    assert result is not None
    assert isinstance(result, dict)
    assert result["opportunity_id"] == "01ARZ3NDEKTSV4RRFFQ69G5FAV"
    assert "scoring_inputs" in result
    assert "evidence" in result
    assert "provenance_chain" in result


@pytest.mark.asyncio
@patch("server.brain.store.get_object")
async def test_explain_returns_none_for_nonexistent(mock_get: AsyncMock) -> None:
    """Explain returns None for an opportunity ID that does not exist."""
    mock_get.return_value = None

    result = await explain("nonexistent-id")
    assert result is None


@pytest.mark.asyncio
@patch("server.brain.store.get_object")
async def test_explain_tree_includes_summary(mock_get: AsyncMock) -> None:
    """Explain tree includes the opportunity summary."""
    summary = "Hawkish Fed signals tighter monetary policy."
    obj = _make_brain_object(summary=summary)
    mock_get.return_value = obj

    result = await explain("01ARZ3NDEKTSV4RRFFQ69G5FAV")
    assert result is not None
    assert result["summary"] == summary


@pytest.mark.asyncio
@patch("server.brain.store.get_object")
async def test_explain_tree_includes_scoring_inputs(mock_get: AsyncMock) -> None:
    """Explain tree includes scoring_inputs derived from object fields."""
    obj = _make_brain_object(
        confidence=0.85,
        entities=["Federal Reserve", "$SPY"],
        market_keys=["SPY-options"],
    )
    mock_get.return_value = obj

    result = await explain("01ARZ3NDEKTSV4RRFFQ69G5FAV")
    assert result is not None
    input_names = {i["name"] for i in result["scoring_inputs"]}
    assert "trust" in input_names
    assert "confidence" in input_names
    assert "kind" in input_names
    assert "entities" in input_names
    assert "market_keys" in input_names


@pytest.mark.asyncio
@patch("server.brain.store.get_object")
async def test_explain_tree_includes_evidence_refs(mock_get: AsyncMock) -> None:
    """Explain tree includes evidence refs from linked_events."""
    obj = _make_brain_object(linked_events=["event_abc123", "event_def456"])
    mock_get.return_value = obj

    result = await explain("01ARZ3NDEKTSV4RRFFQ69G5FAV")
    assert result is not None
    assert len(result["evidence"]) == 2
    refs = {e["ref"] for e in result["evidence"]}
    assert "event_abc123" in refs
    assert "event_def456" in refs


@pytest.mark.asyncio
@patch("server.brain.store.get_object")
async def test_explain_tree_includes_provenance_chain(mock_get: AsyncMock) -> None:
    """Explain tree includes provenance chain with pipeline steps."""
    obj = _make_brain_object()
    mock_get.return_value = obj

    result = await explain("01ARZ3NDEKTSV4RRFFQ69G5FAV")
    assert result is not None
    assert len(result["provenance_chain"]) >= 2
    steps = {s["step"] for s in result["provenance_chain"]}
    assert "ingested" in steps
    assert "indexed" in steps
