"""Telegram alert sender — posts formatted messages with inline action buttons."""

import logging
import os

import httpx

from server.alerts.models import AlertPayload

logger = logging.getLogger(__name__)

TELEGRAM_API = "https://api.telegram.org/bot{token}/sendMessage"


def _format_message(payload: AlertPayload) -> str:
    """Format alert as Telegram markdown message."""
    lines = [
        f"\U0001f4ca *{payload.rule_name}* opportunity — {payload.summary}",
        f"Net edge: {payload.net_edge} | Confidence: {payload.confidence}",
    ]
    return "\n".join(lines)


def _build_keyboard(payload: AlertPayload) -> dict[str, list[list[dict[str, str]]]]:
    """Build inline keyboard markup with action buttons."""
    buttons = []
    for action in payload.inline_actions:
        if action == "simulate":
            label = "\U0001f9ea Simulate"
        else:
            label = "\U0001f4cb Ignore"
        callback_data = f"{action}|{payload.opportunity_id}"
        buttons.append([{"text": label, "callback_data": callback_data}])
    return {"inline_keyboard": buttons}


async def send_alert(payload: AlertPayload) -> str:
    """Send alert to Telegram. Returns message_id string, or '' on failure."""
    token = os.environ.get("AETHER_TELEGRAM_BOT_TOKEN", "")
    chat_id = os.environ.get("AETHER_TELEGRAM_CHAT_ID", "")

    if not token or not chat_id:
        logger.warning(
            "Telegram not configured: missing AETHER_TELEGRAM_BOT_TOKEN "
            "or AETHER_TELEGRAM_CHAT_ID"
        )
        return ""

    text = _format_message(payload)
    reply_markup = _build_keyboard(payload)

    body = {
        "chat_id": chat_id,
        "text": text,
        "parse_mode": "Markdown",
        "reply_markup": reply_markup,
    }

    async with httpx.AsyncClient() as client:
        try:
            resp = await client.post(
                TELEGRAM_API.format(token=token),
                json=body,
                timeout=10.0,
            )
            resp.raise_for_status()
            data = resp.json()
            if data.get("ok") and "result" in data:
                msg_id = str(data["result"].get("message_id", ""))
                logger.info("Telegram alert sent: message_id=%s", msg_id)
                return msg_id

            # Fallback: retry without parse_mode (markdown may fail)
            logger.warning(
                "Telegram API returned not-ok, retrying without parse_mode: %s",
                data,
            )
            body.pop("parse_mode", None)
            fallback_resp = await client.post(
                TELEGRAM_API.format(token=token),
                json=body,
                timeout=10.0,
            )
            fallback_resp.raise_for_status()
            fallback_data = fallback_resp.json()
            if fallback_data.get("ok"):
                return str(fallback_data["result"].get("message_id", ""))
            return ""
        except httpx.HTTPError as exc:
            logger.error("Telegram send failed: %s", exc)
            return ""
