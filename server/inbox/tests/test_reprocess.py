"""Tests for the /inbox/reprocess endpoint."""

from unittest.mock import AsyncMock, patch

import pytest
from fastapi import FastAPI
from httpx import ASGITransport, AsyncClient
from httpx import Response as HttpxResponse

from server.inbox.app import app as inbox_app

# Use the real inbox app for testing the reprocess endpoint
_test_app = FastAPI()
_test_app.mount("/inbox", inbox_app)


@pytest.fixture
def client():
    """Provide an async HTTP test client."""
    transport = ASGITransport(app=inbox_app)
    return AsyncClient(transport=transport, base_url="http://test")


@pytest.fixture(autouse=True)
def dev_auth(monkeypatch: pytest.MonkeyPatch):
    monkeypatch.setenv("AETHER_ENV", "dev")


# ── Tier gating ────────────────────────────────────────────────────────────


@pytest.mark.asyncio
async def test_reprocess_tier_2_rejected(client: AsyncClient) -> None:
    """Tier < 3 returns 403."""
    resp: HttpxResponse = await client.post(
        "/inbox/reprocess",
        json={"object_id": "01ARZ3NDEKTSV4RRFFQ69G5FAV"},
        headers={"Authorization": "Bearer test-viewer"},
    )
    assert resp.status_code == 403
    body = resp.json()
    assert "Tier 3+" in body["error"]


@pytest.mark.asyncio
async def test_reprocess_tier_0_rejected(client: AsyncClient) -> None:
    """Tier 0 returns 403."""
    resp: HttpxResponse = await client.post(
        "/inbox/reprocess",
        json={"object_id": "01ARZ3NDEKTSV4RRFFQ69G5FAV"},
        headers={"Authorization": "Bearer invalid"},
    )
    assert resp.status_code == 401


@pytest.mark.asyncio
async def test_reprocess_no_tier_header_rejected(client: AsyncClient) -> None:
    """Missing authentication returns 401."""
    resp: HttpxResponse = await client.post(
        "/inbox/reprocess",
        json={"object_id": "01ARZ3NDEKTSV4RRFFQ69G5FAV"},
    )
    assert resp.status_code == 401


@pytest.mark.asyncio
async def test_reprocess_rejects_spoofed_tier_header(client: AsyncClient) -> None:
    """A caller cannot self-assert tier 5 without an authenticated session."""
    resp = await client.post(
        "/inbox/reprocess",
        json={"object_id": "01ARZ3NDEKTSV4RRFFQ69G5FAV"},
        headers={"X-Tier": "5"},
    )
    assert resp.status_code == 401


@pytest.mark.asyncio
async def test_reprocess_tier_3_accepted_with_mock(client: AsyncClient) -> None:
    """Tier >= 3 accepted (mock the brain service)."""
    mock_obj = AsyncMock()
    mock_obj.id = "01ARZ3NDEKTSV4RRFFQ69G5FAV"
    mock_obj.kind = type("Kind", (), {"value": "email"})()
    mock_obj.source = "alice@example.com"
    mock_obj.raw_ref = "raw/test/2026/07/12/abc123"

    with (
        patch(
            "server.brain.service.get_by_id",
            return_value=mock_obj,
        ),
        patch("server.brain.storage.get_raw", return_value=b"raw content"),
        patch("server.brain.service.reprocess_object", AsyncMock()),
    ):
        resp: HttpxResponse = await client.post(
            "/inbox/reprocess",
            json={"object_id": "01ARZ3NDEKTSV4RRFFQ69G5FAV"},
            headers={"Authorization": "Bearer test-trader"},
        )
    assert resp.status_code == 200
    body = resp.json()
    assert body["status"] == "ok"
    assert body["original_object_id"] == "01ARZ3NDEKTSV4RRFFQ69G5FAV"


@pytest.mark.asyncio
async def test_reprocess_tier_4_accepted_with_mock(client: AsyncClient) -> None:
    """Tier 4 is also accepted (>= 3)."""
    mock_obj = AsyncMock()
    mock_obj.id = "01ARZ3NDEKTSV4RRFFQ69G5FAV"
    mock_obj.kind = type("Kind", (), {"value": "email"})()
    mock_obj.source = "alice@example.com"
    mock_obj.raw_ref = "raw/test/2026/07/12/abc123"

    with (
        patch(
            "server.brain.service.get_by_id",
            return_value=mock_obj,
        ),
        patch("server.brain.storage.get_raw", return_value=b"raw content"),
        patch("server.brain.service.reprocess_object", AsyncMock()),
    ):
        resp: HttpxResponse = await client.post(
            "/inbox/reprocess",
            json={"object_id": "01ARZ3NDEKTSV4RRFFQ69G5FAV"},
            headers={"Authorization": "Bearer test-admin"},
        )
    assert resp.status_code == 200


# ── Object lookup ──────────────────────────────────────────────────────────


@pytest.mark.asyncio
async def test_reprocess_unknown_object_returns_404(client: AsyncClient) -> None:
    """Unknown object ID returns 404."""
    with patch("server.brain.service.get_by_id", return_value=None):
        resp: HttpxResponse = await client.post(
            "/inbox/reprocess",
            json={"object_id": "01ARZ3NDEKTSV4RRFFQ69G5FAV"},
            headers={"Authorization": "Bearer test-trader"},
        )
    assert resp.status_code == 404
    body = resp.json()
    assert "Object not found" in body["error"]


@pytest.mark.asyncio
async def test_reprocess_missing_object_id(client: AsyncClient) -> None:
    """Missing object_id in body returns 400."""
    resp: HttpxResponse = await client.post(
        "/inbox/reprocess",
        json={},
        headers={"Authorization": "Bearer test-trader"},
    )
    assert resp.status_code == 400
    body = resp.json()
    assert "Missing object_id" in body["error"]


@pytest.mark.asyncio
async def test_reprocess_invalid_json_body(client: AsyncClient) -> None:
    """Invalid JSON body returns 400."""
    resp: HttpxResponse = await client.post(
        "/inbox/reprocess",
        content=b"not-json",
        headers={
            "content-type": "application/json",
            "Authorization": "Bearer test-trader",
        },
    )
    assert resp.status_code == 400
