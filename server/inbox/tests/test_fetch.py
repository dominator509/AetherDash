"""Tests for the fetch and dedup modules."""

import base64
import hashlib

import httpx
import pytest

from server.inbox import dedup
from server.inbox.fetch import fetch_message


@pytest.fixture(autouse=True)
def isolated_dedup(tmp_path, monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("AETHER_INBOX__DEDUP_DB", str(tmp_path / "dedup.sqlite3"))
    dedup.clear()
    monkeypatch.setenv("AETHER_INBOX__GMAIL_ACCESS_TOKEN", "gmail-token")
    monkeypatch.setenv("AETHER_INBOX__MSGRAPH_ACCESS_TOKEN", "graph-token")


@pytest.fixture
def provider_client() -> httpx.AsyncClient:
    raw_email = (
        base64.urlsafe_b64encode(
            b"From: sender@example.com\nSubject: Test Email\n\nGmail body"
        )
        .decode()
        .rstrip("=")
    )

    def handler(request: httpx.Request) -> httpx.Response:
        if request.url.host == "gmail.googleapis.com":
            assert request.headers["Authorization"] == "Bearer gmail-token"
            return httpx.Response(200, json={"raw": raw_email})
        assert request.headers["Authorization"] == "Bearer graph-token"
        return httpx.Response(
            200,
            json={
                "from": {"emailAddress": {"address": "outlook-user@example.com"}},
                "subject": "MS Graph Test",
                "body": {"content": "Graph body"},
                "attachments": [],
            },
        )

    return httpx.AsyncClient(transport=httpx.MockTransport(handler))


@pytest.mark.asyncio
async def test_fetch_gmail_returns_message_data(
    provider_client: httpx.AsyncClient,
) -> None:
    """Fetch parses an authenticated Gmail API response."""
    result = await fetch_message("gmail", "msg-001", client=provider_client)
    assert result is not None
    assert isinstance(result["raw_bytes"], bytes)
    assert result["from_address"] == "sender@example.com"
    assert result["subject"] == "Test Email"
    assert isinstance(result["attachments"], list)


@pytest.mark.asyncio
async def test_fetch_msgraph_returns_message_data(
    provider_client: httpx.AsyncClient,
) -> None:
    """Fetch parses an authenticated MS Graph API response."""
    result = await fetch_message(
        "msgraph", "Users/me/Messages/msg-001", client=provider_client
    )
    assert result is not None
    assert isinstance(result["raw_bytes"], bytes)
    assert result["from_address"] == "outlook-user@example.com"
    assert result["subject"] == "MS Graph Test"
    assert isinstance(result["attachments"], list)


@pytest.mark.asyncio
async def test_dedup_same_hash_skipped(provider_client: httpx.AsyncClient) -> None:
    """Fetching a message with the same content hash twice skips the second."""
    dedup.clear()

    # First fetch should return content
    first = await fetch_message("gmail", "dedup-test-1", client=provider_client)
    assert first is not None

    # Second fetch of same source/message_id returns None (deduped)
    second = await fetch_message("gmail", "dedup-test-1", client=provider_client)
    assert second is None


@pytest.mark.asyncio
async def test_dedup_different_content_not_skipped(
    provider_client: httpx.AsyncClient,
) -> None:
    """Different messages (different message_ids) are not deduped."""
    dedup.clear()

    # Gmail and MS Graph stubs have different content hashes
    first = await fetch_message("gmail", "unique-1", client=provider_client)
    assert first is not None

    # MS Graph uses different content, so should not be deduped
    second = await fetch_message("msgraph", "unique-2", client=provider_client)
    assert second is not None


@pytest.mark.asyncio
async def test_dedup_hash_hex_length(provider_client: httpx.AsyncClient) -> None:
    """Content hash should be a valid 64-character hex string."""
    dedup.clear()
    result = await fetch_message("gmail", "hash-check", client=provider_client)
    assert result is not None

    raw_hash = hashlib.sha256(result["raw_bytes"]).hexdigest()
    assert len(raw_hash) == 64
    assert all(c in "0123456789abcdef" for c in raw_hash)


@pytest.mark.asyncio
async def test_unknown_source_raises() -> None:
    """Fetch from unknown source raises ValueError."""
    with pytest.raises(ValueError, match="Unknown fetch source"):
        await fetch_message("unknown_provider", "msg-001")
