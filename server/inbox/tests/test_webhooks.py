"""Tests for authenticated Gmail and MS Graph webhook receivers."""

import base64
import json

import pytest
from fastapi import FastAPI
from httpx import ASGITransport, AsyncClient
from httpx import Response as HttpxResponse

from server.inbox.queue import claim
from server.inbox.webhooks import gmail, msgraph

# Build a minimal test app with both webhook routers
_test_app = FastAPI()
_test_app.include_router(gmail.router)
_test_app.include_router(msgraph.router)


@pytest.fixture
def client():
    """Provide an async HTTP test client."""
    transport = ASGITransport(app=_test_app)
    return AsyncClient(
        transport=transport,
        base_url="http://test",
        headers={"Authorization": "Bearer signed-google-token"},
    )


@pytest.fixture(autouse=True)
def provider_auth(monkeypatch: pytest.MonkeyPatch, tmp_path):
    monkeypatch.setattr(gmail, "verify_push_token", lambda token: bool(token))
    monkeypatch.setenv("AETHER_INBOX__MSGRAPH_CLIENT_STATE", "aether-inbox-dev")
    monkeypatch.setenv("AETHER_INBOX__QUEUE_DB", str(tmp_path / "queue.sqlite3"))


def gmail_payload(
    *, include_message: bool = True, include_history: bool = True
) -> dict:
    data = {"emailAddress": "operator@example.com"}
    if include_history:
        data["historyId"] = "hist-67890"
    message = {"data": base64.b64encode(json.dumps(data).encode()).decode()}
    if include_message:
        message["messageId"] = "msg-12345"
    return {"message": message, "subscription": "projects/test/subscriptions/sub1"}


# ── Gmail ──────────────────────────────────────────────────────────────────


@pytest.mark.asyncio
async def test_gmail_valid_push_accepted(client: AsyncClient) -> None:
    """Valid Gmail push notification returns 200."""
    payload = gmail_payload()
    resp: HttpxResponse = await client.post("/webhooks/gmail", json=payload)
    assert resp.status_code == 200
    body = resp.json()
    assert body["status"] == "ok"
    queued = claim()
    assert queued is not None and queued.provider == "gmail"


@pytest.mark.asyncio
async def test_gmail_missing_bearer_rejected(client: AsyncClient) -> None:
    """Pub/Sub requests cannot bypass signed OIDC authentication."""
    resp = await client.post(
        "/webhooks/gmail", json=gmail_payload(), headers={"Authorization": ""}
    )
    assert resp.status_code == 401


@pytest.mark.asyncio
async def test_gmail_missing_message_id_rejected(client: AsyncClient) -> None:
    """Gmail push without message_id returns 400 with no body."""
    payload = gmail_payload(include_message=False)
    resp: HttpxResponse = await client.post("/webhooks/gmail", json=payload)
    assert resp.status_code == 400
    assert resp.content == b""


@pytest.mark.asyncio
async def test_gmail_missing_history_id_rejected(client: AsyncClient) -> None:
    """Gmail push without history_id returns 400 with no body."""
    payload = gmail_payload(include_history=False)
    resp: HttpxResponse = await client.post("/webhooks/gmail", json=payload)
    assert resp.status_code == 400
    assert resp.content == b""


@pytest.mark.asyncio
async def test_gmail_empty_body_rejected(client: AsyncClient) -> None:
    """Gmail push with empty JSON body returns 400."""
    resp: HttpxResponse = await client.post("/webhooks/gmail", json={})
    assert resp.status_code == 400
    assert resp.content == b""


@pytest.mark.asyncio
async def test_gmail_malformed_json_rejected(client: AsyncClient) -> None:
    """Gmail push with malformed JSON returns 400 with no body."""
    resp: HttpxResponse = await client.post(
        "/webhooks/gmail",
        content=b"not-json",
        headers={"content-type": "application/json"},
    )
    assert resp.status_code == 400
    assert resp.content == b""


# ── MS Graph ────────────────────────────────────────────────────────────────


@pytest.mark.asyncio
async def test_msgraph_validation_token_echo(client: AsyncClient) -> None:
    """MS Graph subscription verification echoes validationToken."""
    resp: HttpxResponse = await client.post(
        "/webhooks/msgraph?validationToken=abc123token"
    )
    assert resp.status_code == 200
    assert resp.text == "abc123token"
    assert resp.headers["content-type"] == "text/plain; charset=utf-8"


@pytest.mark.asyncio
async def test_msgraph_valid_notification_accepted(client: AsyncClient) -> None:
    """Valid MS Graph change notification with correct clientState."""
    payload = {
        "value": [
            {
                "clientState": "aether-inbox-dev",
                "resource": "Users/me/Messages/msg-001",
                "changeType": "created",
            }
        ],
    }
    resp: HttpxResponse = await client.post("/webhooks/msgraph", json=payload)
    assert resp.status_code == 200
    body = resp.json()
    assert body["status"] == "ok"
    queued = claim()
    assert queued is not None and queued.provider == "msgraph"


@pytest.mark.asyncio
async def test_msgraph_invalid_client_state_rejected(client: AsyncClient) -> None:
    """MS Graph notification with wrong clientState returns 400."""
    payload = {
        "value": [
            {
                "clientState": "wrong-secret",
                "resource": "Users/me/Messages/msg-001",
                "changeType": "created",
            }
        ],
    }
    resp: HttpxResponse = await client.post("/webhooks/msgraph", json=payload)
    assert resp.status_code == 401
    assert resp.content == b""


@pytest.mark.asyncio
async def test_msgraph_missing_value_rejected(client: AsyncClient) -> None:
    """MS Graph notification without value array returns 400."""
    payload: dict[str, object] = {}
    resp: HttpxResponse = await client.post("/webhooks/msgraph", json=payload)
    assert resp.status_code == 400
    assert resp.content == b""


@pytest.mark.asyncio
async def test_msgraph_malformed_json_rejected(client: AsyncClient) -> None:
    """MS Graph with malformed JSON returns 400 with no body."""
    resp: HttpxResponse = await client.post(
        "/webhooks/msgraph",
        content=b"not-json",
        headers={"content-type": "application/json"},
    )
    assert resp.status_code == 400
    assert resp.content == b""


# ── No content in error responses ────────────────────────────────────────────


@pytest.mark.asyncio
async def test_all_error_responses_have_no_body(client: AsyncClient) -> None:
    """All error responses (400) across both webhooks have empty bodies."""
    # Gmail bad JSON
    r1 = await client.post(
        "/webhooks/gmail", content=b"{{{", headers={"content-type": "application/json"}
    )
    assert r1.status_code == 400 and r1.content == b""

    # MS Graph bad JSON
    r2 = await client.post(
        "/webhooks/msgraph",
        content=b"{{{",
        headers={"content-type": "application/json"},
    )
    assert r2.status_code == 400 and r2.content == b""

    # MS Graph wrong clientState
    r3 = await client.post(
        "/webhooks/msgraph",
        json={"value": [{"clientState": "bad", "resource": "x", "changeType": "y"}]},
    )
    assert r3.status_code == 401 and r3.content == b""
