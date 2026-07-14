"""Discord alert sender — posts embeds with action buttons via webhook."""

import logging
import os
from datetime import UTC, datetime

import httpx

from server.alerts.models import AlertPayload

logger = logging.getLogger(__name__)

# Discord embed accent color (blue)
EMBED_COLOR = 0x00AAFF


def _build_embed(payload: AlertPayload) -> dict[str, object]:
    """Build a Discord embed from an alert payload."""
    return {
        "title": f"⚠️ {payload.rule_name} Opportunity",
        "description": payload.summary,
        "color": EMBED_COLOR,
        "fields": [
            {"name": "Net Edge", "value": payload.net_edge, "inline": True},
            {
                "name": "Confidence",
                "value": f"{payload.confidence:.0%}",
                "inline": True,
            },
            {
                "name": "Opportunity ID",
                "value": payload.opportunity_id,
                "inline": False,
            },
        ],
        "timestamp": datetime.now(UTC).isoformat(),
    }


def _build_components(payload: AlertPayload) -> list[dict[str, object]]:
    """Build Discord action row components (buttons)."""
    buttons = []
    for action in payload.inline_actions:
        if action == "simulate":
            label = "Simulate"
            style = 1  # Primary (blue)
        else:
            label = "Ignore"
            style = 4  # Danger (red)
        buttons.append(
            {
                "type": 2,  # Button
                "label": label,
                "style": style,
                "custom_id": f"{action}|{payload.opportunity_id}",
            }
        )
    return [{"type": 1, "components": buttons}]  # ActionRow


async def send_alert(payload: AlertPayload) -> str:
    """Send alert via Discord webhook. Returns 'sent' or '' on failure."""
    webhook_url = os.environ.get("AETHER_DISCORD_WEBHOOK_URL", "")
    if not webhook_url:
        logger.warning("Discord not configured: missing AETHER_DISCORD_WEBHOOK_URL")
        return ""

    embed = _build_embed(payload)
    components = _build_components(payload)

    body = {
        "embeds": [embed],
        "components": components,
    }

    async with httpx.AsyncClient() as client:
        try:
            resp = await client.post(webhook_url, json=body, timeout=10.0)
            resp.raise_for_status()
            logger.info("Discord alert sent to webhook")
            return "sent"
        except httpx.HTTPError as exc:
            logger.error("Discord send failed: %s", exc)
            return ""
