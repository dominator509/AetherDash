"""Gmail webhook handler — receives Pub/Sub push notifications.

Validates the push notification format, extracts key fields, and queues
the notification for later fetch.  Returns 200 immediately (Pub/Sub
expects a fast ack).  Malformed payloads get a 400 with no body.
"""

import base64
import json
import logging
import os

from fastapi import APIRouter, Request, Response

from server.inbox.queue import enqueue

logger = logging.getLogger(__name__)

router = APIRouter()


def verify_push_token(token: str) -> bool:
    """Verify Google's signed push OIDC token and configured service account."""
    try:
        from google.auth.transport import requests as google_requests
        from google.oauth2 import id_token

        claims = id_token.verify_oauth2_token(
            token,
            google_requests.Request(),
            os.environ["AETHER_INBOX__GMAIL_AUDIENCE"],
        )
        return bool(
            claims.get("email_verified")
            and claims.get("email")
            == os.environ["AETHER_INBOX__GMAIL_PUSH_SERVICE_ACCOUNT"]
        )
    except Exception:
        return False


@router.post("/webhooks/gmail")
async def receive_gmail_push(request: Request) -> Response:
    """Receive a Gmail Pub/Sub push notification.

    Expects a JSON body with at least ``message.message_id`` and
    ``message.history_id``.  Returns 200 immediately on success, 400
    on malformed payload (empty body, no content in logs).
    """
    authorization = request.headers.get("Authorization", "")
    if not authorization.startswith("Bearer "):
        return Response(status_code=401)
    if not verify_push_token(authorization.removeprefix("Bearer ")):
        return Response(status_code=401)

    try:
        body = await request.json()
    except Exception:
        return Response(status_code=400)

    message = body.get("message") or {}
    message_id = message.get("messageId")
    try:
        decoded = json.loads(base64.b64decode(message["data"], validate=True))
        history_id = decoded["historyId"]
        email_address = decoded["emailAddress"]
    except (KeyError, ValueError, TypeError, json.JSONDecodeError):
        return Response(status_code=400)
    if not message_id or not history_id or not email_address:
        return Response(status_code=400)

    enqueue(
        "gmail",
        message_id,
        {"history_id": str(history_id), "email_address": email_address},
    )
    logger.debug("Gmail notification queued: message_id=%s", message_id)
    return Response(
        content='{"status":"ok"}',
        media_type="application/json",
        status_code=200,
    )
