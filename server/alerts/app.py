"""FastAPI app for the AETHER Alerts service.

Provides health-check endpoints and an ``/alerts/evaluate`` endpoint for
testing rule evaluation.  A stub background consumer for the ``opps.detected``
bus topic is started on startup (EP-004 bus required for live consumption).
"""

import asyncio
import hmac
import logging
import os
from contextlib import asynccontextmanager
from typing import Literal

from fastapi import FastAPI, Header, HTTPException, Request
from pydantic import BaseModel, Field

from server.alerts.approval_notifications import send_approval_prompt
from server.alerts.approvals import (
    ApprovalAction,
    ApprovalRateLimitError,
    ApprovalService,
    DuplicateApprovalError,
    PostgresApprovalStore,
)
from server.alerts.bus import AlertBus
from server.alerts.dispatch import configure_action_effects, deliver_alert
from server.alerts.effects import action_effects_from_env
from server.alerts.rules import DEFAULT_RULES, evaluate
from server.alerts.webhooks import (
    discord_callback,
    slack_callback,
    telegram_callback,
    twilio_callback,
)

logger = logging.getLogger(__name__)
_bus: AlertBus | None = None
_ready = False
_approval_service: ApprovalService | None = None


class ApprovalRequest(BaseModel):
    actor_id: str = Field(min_length=1)
    action: ApprovalAction
    target_id: str = Field(min_length=1)
    channel: Literal["sms", "email"]


async def _background_consumer() -> None:
    """Consume registered opportunity envelopes and deliver matching alerts."""
    assert _bus is not None
    async for opportunity in _bus.opportunities():
        for rule, reason in await evaluate(opportunity, DEFAULT_RULES):
            await deliver_alert(opportunity, rule, reason, _bus.publish)


@asynccontextmanager
async def lifespan(app: FastAPI):  # noqa: ARG001
    """Start background consumer loop on startup, clean up on shutdown."""
    logger.info("alerts service starting")
    global _bus, _ready, _approval_service  # noqa: PLW0603
    effects = action_effects_from_env()
    configure_action_effects(effects)
    _approval_service = (
        ApprovalService(PostgresApprovalStore(), effects) if effects else None
    )
    task = None
    if os.environ.get("AETHER_ALERTS_BUS_ENABLED", "0") == "1":
        _bus = AlertBus()
        await _bus.start()
        _ready = True
        task = asyncio.create_task(_background_consumer())
    yield
    _ready = False
    if task is not None:
        task.cancel()
        try:
            await task
        except asyncio.CancelledError:
            pass
    if _bus is not None:
        await _bus.stop()
        _bus = None
    configure_action_effects(None)
    _approval_service = None
    logger.info("alerts service shutting down")


app = FastAPI(
    title="AETHER Alerts",
    version="0.1.0",
    lifespan=lifespan,
)


@app.get("/healthz")
async def healthz() -> dict:
    """Liveness check."""
    return {"status": "ok", "service": "alerts"}


@app.get("/readyz")
async def readyz() -> dict:
    """Readiness check."""
    return {
        "status": "ok" if _ready else "degraded",
        "service": "alerts",
        **({} if _ready else {"reason": "bus adapter not configured"}),
    }


@app.post("/callbacks/telegram")
async def telegram_webhook(request: Request) -> dict:
    return await telegram_callback(request)


@app.post("/callbacks/slack")
async def slack_webhook(request: Request) -> dict:
    return await slack_callback(request)


@app.post("/callbacks/discord")
async def discord_webhook(request: Request) -> dict:
    return await discord_callback(request)


@app.post("/callbacks/twilio")
async def twilio_webhook(request: Request) -> dict:
    if _approval_service is None:
        raise HTTPException(503, "approval service is not configured")
    return await twilio_callback(request, _approval_service)


@app.post("/internal/approvals")
async def create_approval(
    request: ApprovalRequest, authorization: str | None = Header(None)
) -> dict:
    """Create and deliver a reference for an already-authenticated action request."""
    expected = os.environ.get("AETHER_ALERTS_INTERNAL_TOKEN", "")
    supplied = authorization.removeprefix("Bearer ") if authorization else ""
    if not expected or not hmac.compare_digest(expected, supplied):
        raise HTTPException(401, "invalid internal service authentication")
    if _approval_service is None:
        raise HTTPException(503, "approval service is not configured")
    try:
        record, reference = await _approval_service.request(
            request.actor_id, request.action, request.target_id, request.channel
        )
    except DuplicateApprovalError as exc:
        raise HTTPException(409, str(exc)) from exc
    except ApprovalRateLimitError as exc:
        raise HTTPException(429, str(exc)) from exc
    try:
        message_id = await send_approval_prompt(record, reference)
    except Exception:
        await _approval_service.fail(record.id)
        raise HTTPException(
            503, "approval notification could not be delivered"
        ) from None
    return {"approval_id": record.id, "status": "pending", "message_id": message_id}


@app.post("/alerts/evaluate")
async def evaluate_opportunity(opportunity: dict) -> dict:
    """Evaluate an opportunity against the default rules (for testing).

    Returns a summary of matching rules.
    """
    matches = await evaluate(opportunity, DEFAULT_RULES)
    results = []
    for rule, reason in matches:
        results.append({"rule_name": rule.name, "reason": reason})
    return {"matches": len(results), "results": results}


# Shortcut: if run directly, serve via uvicorn
if __name__ == "__main__":
    import uvicorn

    uvicorn.run("server.alerts.app:app", host="0.0.0.0", port=8003, reload=True)
