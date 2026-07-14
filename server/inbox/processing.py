"""Durable notification processor from provider cursor to low-trust Brain object."""

import asyncio
import hashlib
import logging
import os

import httpx

from server.inbox.dedup import is_duplicate, mark_seen
from server.inbox.fetch import fetch_message
from server.inbox.filing import file_to_brain
from server.inbox.parse import parse_content
from server.inbox.queue import (
    Notification,
    claim,
    complete,
    fail,
    get_cursor,
    set_cursor,
)

logger = logging.getLogger(__name__)


def _attachment_kind(content_type: str) -> str | None:
    if content_type == "application/pdf":
        return "document"
    if content_type.startswith("image/"):
        return "screenshot"
    if content_type.startswith("text/"):
        return "text"
    return None


async def _file_message(message: dict) -> list[str]:
    content_hash = (
        message.get("content_hash") or hashlib.sha256(message["raw_bytes"]).hexdigest()
    )
    if is_duplicate(content_hash):
        return []
    ids = [
        await file_to_brain(
            "email",
            message["from_address"] or "inbox:unknown",
            message["raw_bytes"],
            parse_content("email", message["raw_bytes"]),
        )
    ]
    for attachment in message.get("attachments", []):
        kind = _attachment_kind(attachment["content_type"])
        if kind is None:
            logger.warning("Unsupported inbox attachment type skipped")
            continue
        raw = attachment["raw_bytes"]
        ids.append(
            await file_to_brain(
                kind,
                message["from_address"] or "inbox:unknown",
                raw,
                parse_content(kind, raw),
            )
        )
    mark_seen(content_hash)
    return ids


async def _gmail_message_ids(
    notification: Notification, client: httpx.AsyncClient
) -> list[str]:
    email_address = notification.payload["email_address"]
    cursor_key = f"gmail:{email_address}"
    cursor = get_cursor(cursor_key) or os.environ.get(
        "AETHER_INBOX__GMAIL_START_HISTORY_ID"
    )
    if not cursor:
        raise RuntimeError("Gmail history cursor is not initialized")
    token = os.environ.get("AETHER_INBOX__GMAIL_ACCESS_TOKEN", "")
    if not token:
        raise RuntimeError("Gmail access token is not configured")
    response = await client.get(
        f"https://gmail.googleapis.com/gmail/v1/users/{email_address}/history",
        params={"startHistoryId": cursor, "historyTypes": "messageAdded"},
        headers={"Authorization": f"Bearer {token}"},
    )
    response.raise_for_status()
    ids: list[str] = []
    for history in response.json().get("history", []):
        ids.extend(
            item["message"]["id"]
            for item in history.get("messagesAdded", [])
            if item.get("message", {}).get("id")
        )
    return list(dict.fromkeys(ids))


async def process_notification(
    notification: Notification, client: httpx.AsyncClient | None = None
) -> list[str]:
    owns_client = client is None
    client = client or httpx.AsyncClient(timeout=20, follow_redirects=False)
    try:
        if notification.provider == "gmail":
            object_ids: list[str] = []
            for message_id in await _gmail_message_ids(notification, client):
                message = await fetch_message(
                    "gmail", message_id, client=client, claim_content=False
                )
                if message is not None:
                    object_ids.extend(await _file_message(message))
            set_cursor(
                f"gmail:{notification.payload['email_address']}",
                notification.payload["history_id"],
            )
            return object_ids
        if notification.provider == "msgraph":
            message = await fetch_message(
                "msgraph",
                notification.payload["resource"],
                client=client,
                claim_content=False,
            )
            return [] if message is None else await _file_message(message)
        raise ValueError("unknown inbox notification provider")
    finally:
        if owns_client:
            await client.aclose()


async def worker_loop(stop: asyncio.Event) -> None:
    """Lease and process notifications until shutdown, preserving retries."""
    while not stop.is_set():
        notification = await asyncio.to_thread(claim)
        if notification is None:
            try:
                await asyncio.wait_for(stop.wait(), timeout=0.5)
            except TimeoutError:
                continue
            continue
        try:
            await process_notification(notification)
        except Exception as exc:
            logger.warning("Inbox notification failed: %s", type(exc).__name__)
            await asyncio.to_thread(
                fail, notification.id, type(exc).__name__, notification.attempts
            )
        else:
            await asyncio.to_thread(complete, notification.id)
