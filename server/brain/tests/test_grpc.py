"""Unit tests for Brain gRPC servicer.

Tests the ``BrainServicer`` methods using mocked service layer.
Since gRPC is a thin wrapper around the service layer, these tests
verify correct request-to-service mapping and error handling.

The proto is compiled at import time by ``grpc_server.py``.
"""

import json
from unittest.mock import AsyncMock, patch

import grpc
import pytest

from server.brain.models import (
    BrainObject,
    BrainRef,
    ObjectDraft,
    ObjectKind,
    Origin,
    Tier,
    TrustLevel,
    now_iso,
)
from server.brain.recall import ScoredRef

# ── Helpers ────────────────────────────────────────────────────────────────


def _make_brain_object(**overrides: object) -> BrainObject:
    """Build a BrainObject with sensible defaults for testing."""
    defaults: dict = {
        "id": "01ARZ3NDEKTSV4RRFFQ69G5FAV",
        "kind": ObjectKind.news,
        "source": "feed://test",
        "origin": Origin.ingest_fleet,
        "trust": TrustLevel.medium,
        "ingested_ts": now_iso(),
        "provenance_hash": "a" * 64,
        "tier": Tier.hot,
        "entities": [],
        "linked_events": [],
        "market_keys": [],
        "summary": "Test object summary.",
    }
    defaults.update(overrides)
    return BrainObject(**defaults)


def _make_grpc_context() -> AsyncMock:
    """Create a mock gRPC context with ``abort`` as an async side effect.

    ``context.abort(status_code, details)`` raises ``grpc.RpcError`` so the
    servicer's return is never reached.  We simulate that via a side effect
    so tests can verify the abort was called.
    """
    ctx = AsyncMock()
    ctx.abort = AsyncMock(side_effect=grpc.RpcError("aborted"))
    return ctx


def _make_store_request(**overrides: object) -> object:
    """Build a Store request object (proto-like) with the given fields."""
    req = AsyncMock()
    req.kind = overrides.get("kind", "note")
    req.content = overrides.get("content", "Test content for gRPC store.")
    req.source = overrides.get("source", "test-grpc")
    return req


def _make_get_request(**overrides: object) -> object:
    """Build a Get request object (proto-like)."""
    req = AsyncMock()
    req.id = overrides.get("id", "01ARZ3NDEKTSV4RRFFQ69G5FAV")
    req.provenance_hash = overrides.get("provenance_hash", "a" * 64)
    return req


def _make_recall_request(**overrides: object) -> object:
    """Build a Recall request object (proto-like)."""
    req = AsyncMock()
    req.query = overrides.get("query", "market conditions")
    req.k = overrides.get("k", 10)
    req.filters = overrides.get("filters", "")
    return req


def _make_explain_request(opportunity_id: str = "01ARZ3NDEKTSV4RRFFQ69G5FAV") -> object:
    """Build an Explain request object (proto-like)."""
    req = AsyncMock()
    req.opportunity_id = AsyncMock()
    req.opportunity_id.value = opportunity_id
    return req


# ═══════════════════════════════════════════════════════════════════════
# Store tests
# ═══════════════════════════════════════════════════════════════════════


@pytest.mark.asyncio
@patch("server.brain.grpc_server.brain_service.store_draft")
async def test_store_returns_brain_ref_with_valid_ulid(mock_store: AsyncMock) -> None:
    """Store → returns BrainRef with valid ULID and provenance hash."""
    from server.brain.grpc_server import BrainServicer, core_pb2

    mock_store.return_value = BrainRef(
        id="01ARZ3NDEKTSV4RRFFQ69G5FAV",
        provenance_hash="a" * 64,
    )

    servicer = BrainServicer()
    request = _make_store_request()
    ctx = _make_grpc_context()

    response = await servicer.Store(request, ctx)

    assert isinstance(response, core_pb2.BrainRef)
    assert response.object_id.value == "01ARZ3NDEKTSV4RRFFQ69G5FAV"
    assert len(response.provenance_hash) == 64
    assert all(c in "0123456789abcdef" for c in response.provenance_hash)

    # Verify service was called with correct draft
    mock_store.assert_awaited_once()
    call_args = mock_store.await_args.args[0]  # type: ignore[union-attr]
    assert isinstance(call_args, ObjectDraft)
    assert call_args.kind == "note"
    assert call_args.content == "Test content for gRPC store."
    assert call_args.source == "test-grpc"


@pytest.mark.asyncio
@patch("server.brain.grpc_server.brain_service.store_draft")
async def test_store_invokes_service_correctly(mock_store: AsyncMock) -> None:
    """Store passes correct ObjectDraft to service."""
    from server.brain.grpc_server import BrainServicer

    mock_store.return_value = BrainRef(
        id="01ARZ3NDEKTSV4RRFFQ69G5FAV",
        provenance_hash="a" * 64,
    )

    servicer = BrainServicer()
    request = _make_store_request(kind="report", content="Q2 earnings analysis.")
    ctx = _make_grpc_context()

    await servicer.Store(request, ctx)

    mock_store.assert_awaited_once()
    # mock.call_args holds (_Call((args, kwargs))); await_args is analogous.
    # .args gives positional args tuple; [0] gives the first positional arg.
    call_args = mock_store.await_args.args[0]  # type: ignore[union-attr]
    assert call_args.kind == "report"
    assert call_args.content == "Q2 earnings analysis."


# ═══════════════════════════════════════════════════════════════════════
# Get tests
# ═══════════════════════════════════════════════════════════════════════


@pytest.mark.asyncio
@patch("server.brain.grpc_server.brain_service.get")
async def test_get_returns_object(mock_get: AsyncMock) -> None:
    """Get → returns ObjectDraft with correct fields."""
    from server.brain.grpc_server import BrainServicer, brain_pb2

    obj = _make_brain_object(summary="Retrieved object summary.")
    mock_get.return_value = obj

    servicer = BrainServicer()
    request = _make_get_request()
    ctx = _make_grpc_context()

    response = await servicer.Get(request, ctx)

    assert isinstance(response, brain_pb2.ObjectDraft)
    assert response.kind == "news"
    assert response.content == "Retrieved object summary."
    assert response.source == "feed://test"


@pytest.mark.asyncio
@patch("server.brain.grpc_server.brain_service.get")
async def test_get_returns_none_for_not_found(mock_get: AsyncMock) -> None:
    """Get for nonexistent object → aborts with NOT_FOUND."""
    from server.brain.grpc_server import BrainServicer

    mock_get.return_value = None

    servicer = BrainServicer()
    request = _make_get_request(id="nonexistent-id")
    ctx = _make_grpc_context()

    with pytest.raises(grpc.RpcError):
        await servicer.Get(request, ctx)

    ctx.abort.assert_awaited_once()
    status_code = ctx.abort.call_args[0][0]
    assert status_code == grpc.StatusCode.NOT_FOUND


# ═══════════════════════════════════════════════════════════════════════
# Recall tests
# ═══════════════════════════════════════════════════════════════════════


@pytest.mark.asyncio
@patch("server.brain.grpc_server.brain_service.recall")
async def test_recall_returns_scored_refs(mock_recall: AsyncMock) -> None:
    """Recall → returns list of ScoredRef with scores."""
    from server.brain.grpc_server import BrainServicer, brain_pb2

    mock_recall.return_value = [
        ScoredRef(
            object_id="01ARZ3NDEKTSV4RRFFQ69G5FAV",
            provenance_hash="a" * 64,
            score=0.042,
        ),
        ScoredRef(
            object_id="01ARZ3NDEKTSV4RRFFQ69G5FBV",
            provenance_hash="b" * 64,
            score=0.021,
        ),
    ]

    servicer = BrainServicer()
    request = _make_recall_request()
    ctx = _make_grpc_context()

    response = await servicer.Recall(request, ctx)

    assert isinstance(response, brain_pb2.RecallResponse)
    assert len(response.refs) == 2

    # Check first ref
    assert response.refs[0].ref.object_id.value == "01ARZ3NDEKTSV4RRFFQ69G5FAV"
    assert response.refs[0].ref.provenance_hash == "a" * 64
    assert abs(response.refs[0].score - 0.042) < 1e-6

    # Check second ref
    assert response.refs[1].ref.object_id.value == "01ARZ3NDEKTSV4RRFFQ69G5FBV"
    assert response.refs[1].ref.provenance_hash == "b" * 64
    assert abs(response.refs[1].score - 0.021) < 1e-6


@pytest.mark.asyncio
@patch("server.brain.grpc_server.brain_service.recall")
async def test_recall_empty_query(mock_recall: AsyncMock) -> None:
    """Recall with empty query returns empty list (no service call)."""
    from server.brain.grpc_server import BrainServicer, brain_pb2

    servicer = BrainServicer()
    request = _make_recall_request(query="")
    ctx = _make_grpc_context()

    response = await servicer.Recall(request, ctx)

    assert isinstance(response, brain_pb2.RecallResponse)
    assert len(response.refs) == 0
    # Service should have been called (recall module itself handles empty query)
    mock_recall.assert_awaited_once()


@pytest.mark.asyncio
@patch("server.brain.grpc_server.brain_service.recall")
async def test_recall_passes_filters(mock_recall: AsyncMock) -> None:
    """Recall passes parsed JSON filters to service layer."""
    from server.brain.grpc_server import BrainServicer

    mock_recall.return_value = []

    servicer = BrainServicer()
    request = _make_recall_request(
        filters=json.dumps({"kind": "news", "trust": "high"})
    )
    ctx = _make_grpc_context()

    await servicer.Recall(request, ctx)

    mock_recall.assert_awaited_once()
    _call_query = mock_recall.await_args[0]
    call_kwargs = mock_recall.await_args[1]
    assert call_kwargs.get("filters") == {"kind": "news", "trust": "high"}
    assert call_kwargs.get("k") == 10
    assert call_kwargs.get("query") == "market conditions"


@pytest.mark.asyncio
@patch("server.brain.grpc_server.brain_service.recall")
async def test_recall_malformed_filters(mock_recall: AsyncMock) -> None:
    """Recall with malformed JSON filters → INVALID_ARGUMENT."""
    from server.brain.grpc_server import BrainServicer

    servicer = BrainServicer()
    request = _make_recall_request(filters="not-valid-json")
    ctx = _make_grpc_context()

    with pytest.raises(grpc.RpcError):
        await servicer.Recall(request, ctx)

    ctx.abort.assert_awaited_once()
    status_code = ctx.abort.call_args[0][0]
    assert status_code == grpc.StatusCode.INVALID_ARGUMENT


@pytest.mark.asyncio
@patch("server.brain.grpc_server.brain_service.recall")
async def test_recall_service_exception_returns_internal(
    mock_recall: AsyncMock,
) -> None:
    """Recall when service raises → INTERNAL error without raw exception text."""
    from server.brain.grpc_server import BrainServicer

    mock_recall.side_effect = RuntimeError("sensitive internal details")

    servicer = BrainServicer()
    request = _make_recall_request()
    ctx = _make_grpc_context()

    with pytest.raises(grpc.RpcError):
        await servicer.Recall(request, ctx)

    ctx.abort.assert_awaited_once()
    status_code = ctx.abort.call_args[0][0]
    details = ctx.abort.call_args[0][1]
    assert status_code == grpc.StatusCode.INTERNAL
    # Must not leak exception text
    assert "sensitive internal details" not in details
    assert "internal error" in details.lower()


# ═══════════════════════════════════════════════════════════════════════
# Explain tests
# ═══════════════════════════════════════════════════════════════════════


@pytest.mark.asyncio
@patch("server.brain.explain.explain")
async def test_explain_returns_tree_json(mock_explain: AsyncMock) -> None:
    """Explain → returns ExplainTree with tree_json."""
    from server.brain.grpc_server import BrainServicer, brain_pb2

    mock_explain.return_value = {
        "opportunity_id": "01ARZ3NDEKTSV4RRFFQ69G5FAV",
        "summary": "Test explanation.",
        "scoring_inputs": [],
        "evidence": [],
        "provenance_chain": [],
    }

    servicer = BrainServicer()
    request = _make_explain_request()
    ctx = _make_grpc_context()

    response = await servicer.Explain(request, ctx)

    assert isinstance(response, brain_pb2.ExplainTree)
    tree = json.loads(response.tree_json)
    assert tree["opportunity_id"] == "01ARZ3NDEKTSV4RRFFQ69G5FAV"
    assert tree["summary"] == "Test explanation."
    assert "scoring_inputs" in tree
    assert "evidence" in tree
    assert "provenance_chain" in tree


@pytest.mark.asyncio
@patch("server.brain.explain.explain")
async def test_explain_not_found(mock_explain: AsyncMock) -> None:
    """Explain for unknown opportunity → NOT_FOUND."""
    from server.brain.grpc_server import BrainServicer

    mock_explain.return_value = None

    servicer = BrainServicer()
    request = _make_explain_request(opportunity_id="nonexistent-id")
    ctx = _make_grpc_context()

    with pytest.raises(grpc.RpcError):
        await servicer.Explain(request, ctx)

    ctx.abort.assert_awaited_once()
    status_code = ctx.abort.call_args[0][0]
    assert status_code == grpc.StatusCode.NOT_FOUND
