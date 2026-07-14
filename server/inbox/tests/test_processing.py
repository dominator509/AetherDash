"""Recovery and effect-level tests for the durable inbox worker."""

import base64
from types import SimpleNamespace
from unittest.mock import AsyncMock, patch

import httpx
import pytest
from fastapi import FastAPI

from server.inbox.processing import process_notification
from server.inbox.queue import claim, complete, enqueue, get_cursor
from server.inbox.webhooks import msgraph


@pytest.fixture(autouse=True)
def isolated_state(tmp_path, monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("AETHER_INBOX__QUEUE_DB", str(tmp_path / "queue.sqlite3"))
    monkeypatch.setenv("AETHER_INBOX__DEDUP_DB", str(tmp_path / "dedup.sqlite3"))
    monkeypatch.setenv("AETHER_INBOX__MSGRAPH_ACCESS_TOKEN", "graph-token")


def test_queue_is_idempotent_and_expired_lease_recovers() -> None:
    assert enqueue("msgraph", "event-1", {"resource": "users/me/messages/1"})
    assert not enqueue("msgraph", "event-1", {"resource": "users/me/messages/1"})
    first = claim(lease_seconds=-1)
    assert first is not None
    recovered = claim()
    assert recovered is not None
    assert recovered.id == first.id
    assert recovered.attempts == 2
    complete(recovered.id)
    assert claim() is None


@pytest.mark.asyncio
async def test_graph_notification_fetches_parses_and_files_low_trust() -> None:
    enqueue("msgraph", "event-1", {"resource": "users/me/messages/1"})
    notification = claim()
    assert notification is not None

    def handler(request: httpx.Request) -> httpx.Response:
        assert request.headers["Authorization"] == "Bearer graph-token"
        return httpx.Response(
            200,
            json={
                "from": {"emailAddress": {"address": "alice@example.com"}},
                "subject": "Signal",
                "body": {"content": "A low-trust catalyst report"},
                "attachments": [],
            },
        )

    client = httpx.AsyncClient(transport=httpx.MockTransport(handler))
    with patch(
        "server.inbox.processing.file_to_brain",
        AsyncMock(return_value="01ARZ3NDEKTSV4RRFFQ69G5FAV"),
    ) as filing:
        ids = await process_notification(notification, client)
    await client.aclose()

    assert ids == ["01ARZ3NDEKTSV4RRFFQ69G5FAV"]
    filing.assert_awaited_once()
    args = filing.await_args.args
    assert args[0] == "email"
    assert args[1] == "alice@example.com"
    assert "catalyst" in args[3]


@pytest.mark.asyncio
async def test_graph_webhook_to_low_trust_brain_draft(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Provider stub round-trips through durable queue, parser, and Brain.Store."""
    monkeypatch.setenv("AETHER_INBOX__MSGRAPH_CLIENT_STATE", "state-secret")
    app = FastAPI()
    app.include_router(msgraph.router)
    webhook_client = httpx.AsyncClient(
        transport=httpx.ASGITransport(app=app), base_url="http://test"
    )
    response = await webhook_client.post(
        "/webhooks/msgraph",
        json={
            "value": [
                {
                    "id": "event-e2e",
                    "clientState": "state-secret",
                    "resource": "users/me/messages/e2e",
                    "changeType": "created",
                }
            ]
        },
    )
    await webhook_client.aclose()
    assert response.status_code == 200
    notification = claim()
    assert notification is not None

    provider = httpx.AsyncClient(
        transport=httpx.MockTransport(
            lambda _request: httpx.Response(
                200,
                json={
                    "from": {"emailAddress": {"address": "alice@example.com"}},
                    "subject": "Catalyst",
                    "body": {"content": "Untrusted forwarded research"},
                    "attachments": [],
                },
            )
        )
    )
    with patch(
        "server.brain.service.store_draft",
        AsyncMock(return_value=SimpleNamespace(id="01ARZ3NDEKTSV4RRFFQ69G5FAV")),
    ) as store:
        ids = await process_notification(notification, provider)
    await provider.aclose()

    assert ids == ["01ARZ3NDEKTSV4RRFFQ69G5FAV"]
    kwargs = store.await_args.kwargs
    assert kwargs["origin"] == "inbox"
    assert kwargs["trust"] == "low"
    assert kwargs["raw_content"] == b"Untrusted forwarded research"


@pytest.mark.asyncio
async def test_gmail_history_cursor_advances_after_success(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setenv("AETHER_INBOX__GMAIL_ACCESS_TOKEN", "gmail-token")
    monkeypatch.setenv("AETHER_INBOX__GMAIL_START_HISTORY_ID", "100")
    enqueue(
        "gmail",
        "pubsub-1",
        {"email_address": "me@example.com", "history_id": "101"},
    )
    notification = claim()
    assert notification is not None
    raw = (
        base64.urlsafe_b64encode(
            b"From: bob@example.com\nSubject: Filing\n\nNew filing body"
        )
        .decode()
        .rstrip("=")
    )

    def handler(request: httpx.Request) -> httpx.Response:
        if request.url.path.endswith("/history"):
            assert request.url.params["startHistoryId"] == "100"
            return httpx.Response(
                200,
                json={"history": [{"messagesAdded": [{"message": {"id": "m1"}}]}]},
            )
        return httpx.Response(200, json={"raw": raw})

    client = httpx.AsyncClient(transport=httpx.MockTransport(handler))
    with patch(
        "server.inbox.processing.file_to_brain",
        AsyncMock(return_value="01ARZ3NDEKTSV4RRFFQ69G5FAV"),
    ):
        await process_notification(notification, client)
    await client.aclose()
    assert get_cursor("gmail:me@example.com") == "101"


@pytest.mark.asyncio
async def test_gmail_cursor_does_not_advance_when_filing_fails(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setenv("AETHER_INBOX__GMAIL_ACCESS_TOKEN", "gmail-token")
    monkeypatch.setenv("AETHER_INBOX__GMAIL_START_HISTORY_ID", "200")
    enqueue(
        "gmail", "pubsub-2", {"email_address": "me@example.com", "history_id": "201"}
    )
    notification = claim()
    raw = base64.urlsafe_b64encode(b"From: b@example.com\n\nbody").decode().rstrip("=")

    def handler(request: httpx.Request) -> httpx.Response:
        if request.url.path.endswith("/history"):
            return httpx.Response(
                200, json={"history": [{"messagesAdded": [{"message": {"id": "m2"}}]}]}
            )
        return httpx.Response(200, json={"raw": raw})

    client = httpx.AsyncClient(transport=httpx.MockTransport(handler))
    with patch(
        "server.inbox.processing.file_to_brain",
        AsyncMock(side_effect=RuntimeError("down")),
    ):
        with pytest.raises(RuntimeError, match="down"):
            await process_notification(notification, client)
    await client.aclose()
    assert get_cursor("gmail:me@example.com") is None
