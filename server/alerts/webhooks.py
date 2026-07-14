"""Authenticated webhook parsing for alert channel interactions."""

import hashlib
import hmac
import json
import os
import time
from urllib.parse import parse_qs

from fastapi import HTTPException, Request

from server.alerts.dispatch import process_action_callback
from server.alerts.identity import resolve_channel_actor


def _split_action(value: str) -> tuple[str, str]:
    try:
        action, opportunity_id = value.split("|", 1)
    except ValueError as exc:
        raise HTTPException(400, "invalid action payload") from exc
    if action not in {"simulate", "execute", "ignore"} or not opportunity_id:
        raise HTTPException(400, "invalid action payload")
    return action, opportunity_id


async def _dispatch(channel: str, user_id: str, value: str) -> dict:
    grant = await resolve_channel_actor(channel, user_id)
    if grant is None:
        raise HTTPException(403, "unmapped or expired operator")
    action, opportunity_id = _split_action(value)
    return await process_action_callback(
        action, opportunity_id, grant.actor_id, grant.tier
    )


async def telegram_callback(request: Request) -> dict:
    secret = os.environ.get("AETHER_TELEGRAM_WEBHOOK_SECRET", "")
    supplied = request.headers.get("X-Telegram-Bot-Api-Secret-Token", "")
    if not secret or not hmac.compare_digest(secret, supplied):
        raise HTTPException(401, "invalid webhook signature")
    body = await request.json()
    try:
        callback = body["callback_query"]
        return await _dispatch(
            "telegram", str(callback["from"]["id"]), callback["data"]
        )
    except (KeyError, TypeError) as exc:
        raise HTTPException(400, "invalid callback payload") from exc


async def slack_callback(request: Request) -> dict:
    body = await request.body()
    timestamp = request.headers.get("X-Slack-Request-Timestamp", "")
    try:
        if abs(time.time() - int(timestamp)) > 300:
            raise HTTPException(401, "stale webhook")
    except ValueError as exc:
        raise HTTPException(401, "invalid webhook timestamp") from exc
    secret = os.environ.get("AETHER_SLACK_SIGNING_SECRET", "")
    expected = (
        "v0="
        + hmac.new(
            secret.encode(), b"v0:" + timestamp.encode() + b":" + body, hashlib.sha256
        ).hexdigest()
    )
    if not secret or not hmac.compare_digest(
        expected, request.headers.get("X-Slack-Signature", "")
    ):
        raise HTTPException(401, "invalid webhook signature")
    try:
        payload = json.loads(parse_qs(body.decode())["payload"][0])
        return await _dispatch(
            "slack", str(payload["user"]["id"]), payload["actions"][0]["value"]
        )
    except (KeyError, IndexError, TypeError, json.JSONDecodeError) as exc:
        raise HTTPException(400, "invalid callback payload") from exc


async def discord_callback(request: Request) -> dict:
    body = await request.body()
    signature = request.headers.get("X-Signature-Ed25519", "")
    timestamp = request.headers.get("X-Signature-Timestamp", "")
    public_key = os.environ.get("AETHER_DISCORD_PUBLIC_KEY", "")
    try:
        from cryptography.exceptions import InvalidSignature
        from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey

        Ed25519PublicKey.from_public_bytes(bytes.fromhex(public_key)).verify(
            bytes.fromhex(signature), timestamp.encode() + body
        )
    except (ValueError, TypeError, InvalidSignature) as exc:
        raise HTTPException(401, "invalid webhook signature") from exc
    payload = json.loads(body)
    if payload.get("type") == 1:
        return {"type": 1}
    try:
        result = await _dispatch(
            "discord",
            str(payload["member"]["user"]["id"]),
            payload["data"]["custom_id"],
        )
    except (KeyError, TypeError) as exc:
        raise HTTPException(400, "invalid callback payload") from exc
    return {"type": 4, "data": {"content": result["reason"], "flags": 64}}
