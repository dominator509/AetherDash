"""Slack alert sender — posts Block Kit messages with action buttons via webhook."""

import logging
import os

import httpx

from server.alerts.models import AlertPayload

logger = logging.getLogger(__name__)


def _build_blocks(payload: AlertPayload) -> list[dict[str, object]]:
    """Build Slack Block Kit blocks from an alert payload."""
    blocks: list[dict[str, object]] = []

    # Header block
    blocks.append(
        {
            "type": "header",
            "text": {
                "type": "plain_text",
                "text": f"⚠️ {payload.rule_name} Opportunity",
                "emoji": True,
            },
        }
    )

    # Section block with summary and fields
    blocks.append(
        {
            "type": "section",
            "text": {
                "type": "mrkdwn",
                "text": payload.summary,
            },
            "fields": [
                {"type": "mrkdwn", "text": f"*Net Edge:*\n{payload.net_edge}"},
                {
                    "type": "mrkdwn",
                    "text": f"*Confidence:*\n{payload.confidence:.0%}",
                },
            ],
        }
    )

    # Divider
    blocks.append({"type": "divider"})

    # Action buttons
    buttons = []
    for action in payload.inline_actions:
        label = "Simulate" if action == "simulate" else "Ignore"
        buttons.append(
            {
                "type": "button",
                "text": {"type": "plain_text", "text": label, "emoji": True},
                "value": f"{action}|{payload.opportunity_id}",
                "action_id": f"{action}_{payload.opportunity_id}",
            }
        )

    blocks.append(
        {
            "type": "actions",
            "elements": buttons,
        }
    )

    return blocks


async def send_alert(payload: AlertPayload) -> str:
    """Send alert via Slack webhook. Returns 'sent' or '' on failure."""
    webhook_url = os.environ.get("AETHER_SLACK_WEBHOOK_URL", "")
    if not webhook_url:
        logger.warning("Slack not configured: missing AETHER_SLACK_WEBHOOK_URL")
        return ""

    blocks = _build_blocks(payload)

    body = {
        "text": f"Alert: {payload.rule_name}",  # fallback text for notifications
        "blocks": blocks,
    }

    async with httpx.AsyncClient() as client:
        try:
            resp = await client.post(webhook_url, json=body, timeout=10.0)
            resp.raise_for_status()
            logger.info("Slack alert sent to webhook")
            return "sent"
        except httpx.HTTPError as exc:
            logger.error("Slack send failed: %s", exc)
            return ""
