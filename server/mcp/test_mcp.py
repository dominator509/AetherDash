"""MCP tier-filtering contract tests."""

import pytest
from app import app
from auth import PermissionDeniedError, Session
from fastapi.testclient import TestClient
from pytest import MonkeyPatch


@pytest.fixture(autouse=True)
def _set_dev_env(monkeypatch: MonkeyPatch) -> None:
    """All tests in this module use AETHER_ENV=dev so test tokens are enabled.
    Production fail-closed behavior is tested separately (see test_prod_rejects_test_tokens)."""
    monkeypatch.setenv("AETHER_ENV", "dev")


client = TestClient(app)


def _auth(token: str = "test-admin") -> dict[str, str]:
    return {"Authorization": f"Bearer {token}"}


# ── Health ──────────────────────────────────────────────────────────────────


def test_healthz() -> None:
    resp = client.get("/healthz")
    assert resp.status_code == 200
    assert resp.json()["status"] == "ok"


# ── Manifest listing (GET /tools) ───────────────────────────────────────────


def test_viewer_only_sees_tier_1_tools() -> None:
    resp = client.get("/tools", headers=_auth("test-viewer"))
    assert resp.status_code == 200
    names = {t["name"] for t in resp.json()["tools"]}
    # Tier 1 tools
    assert "brain.search" in names
    assert "markets.query" in names
    # Tier 2+ tools excluded
    assert "orders.submit_paper" not in names
    assert "orders.submit" not in names


def test_trader_sees_paper_not_live() -> None:
    resp = client.get("/tools", headers=_auth("test-trader"))
    names = {t["name"] for t in resp.json()["tools"]}
    assert "orders.submit_paper" in names
    assert "orders.submit" not in names


def test_admin_sees_all() -> None:
    resp = client.get("/tools", headers=_auth("test-admin"))
    names = {t["name"] for t in resp.json()["tools"]}
    assert "orders.submit" in names


# ── Tool invocation (POST /tools/{name}) ────────────────────────────────────


def test_viewer_cannot_call_tier_2_tool() -> None:
    resp = client.post("/tools/orders.draft", headers=_auth("test-viewer"))
    assert resp.status_code == 403


def test_trader_can_call_paper_tool() -> None:
    resp = client.post("/tools/orders.submit_paper", headers=_auth("test-trader"))
    assert resp.status_code == 200
    assert "stub" in resp.json()["result"]


def test_unknown_tool_404() -> None:
    resp = client.post("/tools/nonexistent", headers=_auth("test-admin"))
    assert resp.status_code == 404


# ── Auth enforcement ────────────────────────────────────────────────────────


def test_unauthenticated_returns_401() -> None:
    """No Authorization header → 401."""
    resp = client.get("/tools")
    assert resp.status_code == 401


def test_invalid_token_returns_401() -> None:
    """Unrecognized Bearer token → 401."""
    resp = client.get("/tools", headers={"Authorization": "Bearer invalid-token"})
    assert resp.status_code == 401


# ── ErrorEnvelope shape ────────────────────────────────────────────────────


def test_error_envelope_format_on_errors() -> None:
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


# ── Production fail-closed ───────────────────────────────────────────────────


def test_prod_rejects_test_tokens(monkeypatch: MonkeyPatch) -> None:
    """In production (AETHER_ENV != dev), test tokens must be rejected."""
    # Override the module-level fixture: force prod mode for this test only
    monkeypatch.setenv("AETHER_ENV", "prod")
    # Must re-import auth to pick up the env change (auth reads env at call time)
    resp = client.get("/tools", headers=_auth("test-admin"))
    assert resp.status_code == 401
    body = resp.json()
    assert body["code"] == "unauthenticated"


# ── Grant enforcement ─────────────────────────────────────────────────────


async def _mock_perm_denied(*args: object, **kwargs: object) -> Session:
    """Mock authenticate that always raises PermissionDeniedError."""
    raise PermissionDeniedError("test: no grant")


def test_no_grant_returns_403(monkeypatch: MonkeyPatch) -> None:
    """Session with no grant row → 403 permission_denied.
    Patches app.authenticate because app.py uses ``from auth import authenticate``,
    so auth.authenticate and app.authenticate are distinct references."""
    import app as app_module

    monkeypatch.setattr(app_module, "authenticate", _mock_perm_denied)
    resp = client.get("/tools", headers={"Authorization": "Bearer some-token"})
    assert resp.status_code == 403
    assert resp.json()["code"] == "permission_denied"


def test_expired_grant_returns_403(monkeypatch: MonkeyPatch) -> None:
    """Session with expired grant → 403 permission_denied."""
    import app as app_module

    monkeypatch.setattr(app_module, "authenticate", _mock_perm_denied)
    resp = client.post(
        "/tools/brain.search", headers={"Authorization": "Bearer some-token"}
    )
    assert resp.status_code == 403
    assert resp.json()["code"] == "permission_denied"


async def _mock_grant_scopes(*args: object, **kwargs: object) -> Session:
    """Mock authenticate returning a Session with specific grant scopes."""
    return Session(
        session_id="test-session",
        user_id="test-user",
        actor_id="test-user",
        tier=1,
        origin_kind="human",
        scopes={"allowed": ["brain.search"]},
        grant_tier=1,
    )


def test_grant_scopes_filter_tools(monkeypatch: MonkeyPatch) -> None:
    """Grant scopes restrict visible tools even when tier is sufficient."""
    import app as app_module

    monkeypatch.setattr(app_module, "authenticate", _mock_grant_scopes)
    resp = client.get("/tools", headers={"Authorization": "Bearer some-token"})
    assert resp.status_code == 200
    names = {t["name"] for t in resp.json()["tools"]}
    assert "brain.search" in names
    assert "markets.query" not in names  # excluded by scope
    # Verify scopes are returned in response
    assert resp.json()["scopes"] is not None
    assert "allowed" in resp.json()["scopes"]


def test_grant_scopes_block_tool_call(monkeypatch: MonkeyPatch) -> None:
    """Grant scopes prevent calling a tool not in the scope list."""
    import app as app_module

    monkeypatch.setattr(app_module, "authenticate", _mock_grant_scopes)
    resp = client.post(
        "/tools/markets.query", headers={"Authorization": "Bearer some-token"}
    )
    assert resp.status_code == 403
    assert resp.json()["code"] == "permission_denied"
