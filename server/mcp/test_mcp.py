"""MCP tier-filtering contract tests."""

import pytest
from fastapi.testclient import TestClient

from app import app

client = TestClient(app)


def _auth(token: str = "test-admin") -> dict:
    return {"Authorization": f"Bearer {token}"}


# ── Health ──────────────────────────────────────────────────────────────────


def test_healthz():
    resp = client.get("/healthz")
    assert resp.status_code == 200
    assert resp.json()["status"] == "ok"


# ── Manifest listing (GET /tools) ───────────────────────────────────────────


def test_viewer_only_sees_tier_1_tools():
    resp = client.get("/tools", headers=_auth("test-viewer"))
    assert resp.status_code == 200
    names = {t["name"] for t in resp.json()["tools"]}
    # Tier 1 tools
    assert "brain.search" in names
    assert "markets.query" in names
    # Tier 2+ tools excluded
    assert "orders.submit_paper" not in names
    assert "orders.submit" not in names


def test_trader_sees_paper_not_live():
    resp = client.get("/tools", headers=_auth("test-trader"))
    names = {t["name"] for t in resp.json()["tools"]}
    assert "orders.submit_paper" in names
    assert "orders.submit" not in names


def test_admin_sees_all():
    resp = client.get("/tools", headers=_auth("test-admin"))
    names = {t["name"] for t in resp.json()["tools"]}
    assert "orders.submit" in names


# ── Tool invocation (POST /tools/{name}) ────────────────────────────────────


def test_viewer_cannot_call_tier_2_tool():
    resp = client.post("/tools/orders.draft", headers=_auth("test-viewer"))
    assert resp.status_code == 403


def test_trader_can_call_paper_tool():
    resp = client.post("/tools/orders.submit_paper", headers=_auth("test-trader"))
    assert resp.status_code == 200
    assert "stub" in resp.json()["result"]


def test_unknown_tool_404():
    resp = client.post("/tools/nonexistent", headers=_auth("test-admin"))
    assert resp.status_code == 404


# ── Auth enforcement ────────────────────────────────────────────────────────


def test_unauthenticated_returns_401():
    """No Authorization header → 401."""
    resp = client.get("/tools")
    assert resp.status_code == 401


def test_invalid_token_returns_401():
    """Unrecognized Bearer token → 401."""
    resp = client.get("/tools", headers={"Authorization": "Bearer invalid-token"})
    assert resp.status_code == 401


# ── ErrorEnvelope shape ────────────────────────────────────────────────────


def test_error_envelope_format_on_errors():
    """All error responses carry a valid ErrorEnvelope body."""

    # 401 — unauthenticated
    resp = client.get("/tools")
    assert resp.status_code == 401
    body = resp.json()
    assert body["code"] == "unauthenticated"
    assert "message" in body
    assert body["retryable"] is False
    assert "trace_id" in body

    # 403 — permission denied
    resp = client.post("/tools/orders.draft", headers=_auth("test-viewer"))
    assert resp.status_code == 403
    body = resp.json()
    assert body["code"] == "permission_denied"
    assert "message" in body
    assert body["retryable"] is False
    assert "trace_id" in body

    # 404 — not found
    resp = client.post("/tools/nonexistent", headers=_auth("test-admin"))
    assert resp.status_code == 404
    body = resp.json()
    assert body["code"] == "not_found"
    assert "message" in body
    assert body["retryable"] is False
    assert "trace_id" in body
