"""MS Graph webhook handler — receives subscription change notifications.

Handles two flows:
1. Subscription verification (``validationToken`` query param) — echoes the
   token as ``text/plain``.
2. Regular change notifications — validates ``clientState`` and queues for
   later fetch.

Returns 200 immediately for valid payloads, 400 for malformed or
unauthenticated requests (empty body, no content in logs).
"""

import hmac
import logging
import os

from fastapi import APIRouter, Request, Response

from server.inbox.queue import enqueue

logger = logging.getLogger(__name__)

router = APIRouter()


@router.post("/webhooks/msgraph")
async def receive_msgraph_notification(request: Request) -> Response:
    """Receive an MS Graph subscription change notification.

    If the request has a ``validationToken`` query parameter, treat it as
    a subscription verification and echo the token back.

    Otherwise validate ``clientState`` and queue the notification for fetch.
    """
    # --- Subscription verification ---
    validation_token = request.query_params.get("validationToken")
    if validation_token:
        logger.debug("MS Graph subscription verification requested")
        return Response(
            content=validation_token,
            media_type="text/plain",
            status_code=200,
        )

    # --- Change notification ---
    try:
        body = await request.json()
    except Exception:
        return Response(status_code=400)

    value = body.get("value")
    if not value or not isinstance(value, list):
        return Response(status_code=400)

    expected_state = os.environ.get("AETHER_INBOX__MSGRAPH_CLIENT_STATE", "")
    if not expected_state or any(
        not hmac.compare_digest(str(entry.get("clientState", "")), expected_state)
        for entry in value
    ):
        logger.warning("MS Graph notification with invalid clientState")
        return Response(status_code=401)

    queued = 0
    for entry in value:
        resource = entry.get("resource")
        change_type = entry.get("changeType")
        if not resource or not change_type:
            continue
        event_key = str(
            entry.get("id")
            or f"{entry.get('subscriptionId', '')}:{resource}:{change_type}"
        )
        enqueue(
            "msgraph",
            event_key,
            {"resource": resource, "change_type": change_type},
        )
        queued += 1
        logger.debug(
            "MS Graph notification queued: resource=%s type=%s",
            resource,
            change_type,
        )

    if queued == 0:
        return Response(status_code=400)
    return Response(
        content='{"status":"ok"}',
        media_type="application/json",
        status_code=200,
    )
