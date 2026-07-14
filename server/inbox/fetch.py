"""Authenticated Gmail and Microsoft Graph message fetch adapters."""

import base64
import hashlib
import os
from email import policy
from email.parser import BytesParser

import httpx

from server.inbox.dedup import mark_seen


def _token(name: str) -> str:
    value = os.environ.get(name, "")
    if not value:
        raise RuntimeError(f"required provider credential is not configured: {name}")
    return value


def _claim(raw_bytes: bytes) -> bool:
    return mark_seen(hashlib.sha256(raw_bytes).hexdigest())


async def fetch_message(
    source: str,
    message_id: str,
    *,
    client: httpx.AsyncClient | None = None,
    claim_content: bool = True,
) -> dict | None:
    """Fetch one provider message and atomically content-deduplicate it."""
    owns_client = client is None
    client = client or httpx.AsyncClient(timeout=20, follow_redirects=False)
    try:
        if source == "gmail":
            result = await _fetch_gmail(message_id, client)
        elif source == "msgraph":
            result = await _fetch_msgraph(message_id, client)
        else:
            raise ValueError(f"Unknown fetch source: {source}")
    finally:
        if owns_client:
            await client.aclose()
    if claim_content and not _claim(result["raw_bytes"]):
        return None
    result["content_hash"] = hashlib.sha256(result["raw_bytes"]).hexdigest()
    return result


async def _fetch_gmail(message_id: str, client: httpx.AsyncClient) -> dict:
    token = _token("AETHER_INBOX__GMAIL_ACCESS_TOKEN")
    response = await client.get(
        f"https://gmail.googleapis.com/gmail/v1/users/me/messages/{message_id}",
        params={"format": "raw"},
        headers={"Authorization": f"Bearer {token}"},
    )
    response.raise_for_status()
    encoded = response.json().get("raw", "")
    try:
        raw_bytes = base64.urlsafe_b64decode(encoded + "=" * (-len(encoded) % 4))
        message = BytesParser(policy=policy.default).parsebytes(raw_bytes)
    except Exception as exc:
        raise ValueError("Gmail returned an invalid raw message") from exc
    attachments = [
        {
            "filename": part.get_filename(),
            "content_type": part.get_content_type(),
            "raw_bytes": part.get_payload(decode=True) or b"",
        }
        for part in message.iter_attachments()
    ]
    return {
        "raw_bytes": raw_bytes,
        "from_address": str(message.get("from", "")),
        "subject": str(message.get("subject", "")),
        "attachments": attachments,
    }


async def _fetch_msgraph(resource: str, client: httpx.AsyncClient) -> dict:
    token = _token("AETHER_INBOX__MSGRAPH_ACCESS_TOKEN")
    safe_resource = resource.lstrip("/")
    response = await client.get(
        f"https://graph.microsoft.com/v1.0/{safe_resource}",
        params={"$expand": "attachments"},
        headers={"Authorization": f"Bearer {token}"},
    )
    response.raise_for_status()
    payload = response.json()
    body = payload.get("body", {}).get("content", "")
    raw_bytes = body.encode("utf-8")
    attachments = []
    for item in payload.get("attachments", []):
        encoded = item.get("contentBytes")
        if encoded:
            attachments.append(
                {
                    "filename": item.get("name", ""),
                    "content_type": item.get("contentType", "application/octet-stream"),
                    "raw_bytes": base64.b64decode(encoded, validate=True),
                }
            )
    return {
        "raw_bytes": raw_bytes,
        "from_address": payload.get("from", {})
        .get("emailAddress", {})
        .get("address", ""),
        "subject": payload.get("subject", ""),
        "attachments": attachments,
    }
