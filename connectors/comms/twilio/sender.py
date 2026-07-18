"""Thin Twilio Messages API adapter."""

from __future__ import annotations

import os

import httpx

from server.alerts.models import AlertPayload


class TwilioConfigError(RuntimeError):
    """Required SMS transport configuration is unavailable."""


def format_sms(payload: AlertPayload) -> str:
    """Render a bounded plain-text alert without secret material."""
    text = f"AETHER {payload.rule_name}: {payload.summary} Actions: SIMULATE / EXECUTE / IGNORE"
    return text[:1500]


async def send_alert(
    payload: AlertPayload, *, client: httpx.AsyncClient | None = None
) -> str:
    return await send_message(format_sms(payload), client=client)


async def send_message(message: str, *, client: httpx.AsyncClient | None = None) -> str:
    sid = os.environ.get("AETHER_COMMS__TWILIO_SID", "")
    token = os.environ.get("AETHER_COMMS__TWILIO_TOKEN", "")
    from_number = os.environ.get("AETHER_COMMS__TWILIO_FROM", "")
    to_number = os.environ.get("AETHER_COMMS__TWILIO_TO", "")
    if not all((sid, token, from_number, to_number)):
        raise TwilioConfigError("Twilio SMS transport is not configured")

    owns_client = client is None
    client = client or httpx.AsyncClient(timeout=15, follow_redirects=False)
    try:
        response = await client.post(
            f"https://api.twilio.com/2010-04-01/Accounts/{sid}/Messages.json",
            data={"From": from_number, "To": to_number, "Body": message[:1500]},
            auth=httpx.BasicAuth(sid, token),
        )
        response.raise_for_status()
        body = response.json()
    finally:
        if owns_client:
            await client.aclose()
    message_id = body.get("sid") if isinstance(body, dict) else None
    if not isinstance(message_id, str) or not message_id:
        raise TwilioConfigError("Twilio response did not contain a message id")
    return message_id
