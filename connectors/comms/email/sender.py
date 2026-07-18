"""TLS SMTP alert sender."""

from __future__ import annotations

import asyncio
import os
import smtplib
from email.message import EmailMessage

from server.alerts.models import AlertPayload


class EmailConfigError(RuntimeError):
    """Required email transport configuration is unavailable."""


def format_email(payload: AlertPayload) -> EmailMessage:
    message = EmailMessage()
    message["Subject"] = f"AETHER alert: {payload.rule_name}"
    message.set_content(
        "\n".join(
            [
                payload.summary,
                f"Opportunity: {payload.opportunity_id}",
                f"Net edge: {payload.net_edge}",
                f"Confidence: {payload.confidence:.2f}",
                "Open AETHER to simulate, execute, or ignore this alert.",
            ]
        )
    )
    return message


def _send(message: EmailMessage) -> str:
    host = os.environ.get("AETHER_COMMS__SMTP_HOST", "")
    port = int(os.environ.get("AETHER_COMMS__SMTP_PORT", "587"))
    username = os.environ.get("AETHER_COMMS__SMTP_USERNAME", "")
    password = os.environ.get("AETHER_COMMS__SMTP_PASSWORD", "")
    from_address = os.environ.get("AETHER_COMMS__EMAIL_FROM", "")
    to_address = os.environ.get("AETHER_COMMS__EMAIL_TO", "")
    if not all((host, username, password, from_address, to_address)):
        raise EmailConfigError("SMTP transport is not configured")
    message["From"] = from_address
    message["To"] = to_address
    with smtplib.SMTP(host, port, timeout=15) as smtp:
        smtp.starttls()
        smtp.login(username, password)
        refused = smtp.send_message(message)
    if refused:
        raise EmailConfigError("SMTP server refused one or more recipients")
    return message.get("Message-ID", "smtp-accepted")


async def send_alert(payload: AlertPayload) -> str:
    return await asyncio.to_thread(_send, format_email(payload))


async def send_message(subject: str, body: str) -> str:
    message = EmailMessage()
    message["Subject"] = subject
    message.set_content(body)
    return await asyncio.to_thread(_send, message)
