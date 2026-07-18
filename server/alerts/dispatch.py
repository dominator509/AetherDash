"""Alert dispatch — create AlertMsg and publish to ``alerts.outbound`` bus topic."""

import logging
from collections.abc import Awaitable, Callable
from typing import TYPE_CHECKING, Any, Protocol

import ulid

from server.alerts.actions import InlineAction, handle_action
from server.alerts.effects import ActionEffectError
from server.alerts.history import record_alert, update_delivery
from server.alerts.models import AlertMsg, AlertPayload, now_iso

if TYPE_CHECKING:
    from server.alerts.rules import Rule

logger = logging.getLogger(__name__)


class ActionEffects(Protocol):
    """Existing system seams used after a policy verdict permits an action."""

    async def simulate(self, opportunity_id: str, actor_id: str) -> dict[str, Any]: ...

    async def ignore(self, opportunity_id: str, actor_id: str) -> dict[str, Any]: ...

    async def execute_paper(
        self, opportunity_id: str, actor_id: str, approval_id: str | None = None
    ) -> dict[str, Any]: ...


_action_effects: ActionEffects | None = None


def configure_action_effects(effects: ActionEffects | None) -> None:
    """Install the gateway/router effects adapter during service startup."""
    global _action_effects  # noqa: PLW0603
    _action_effects = effects


async def dispatch_to_channel(
    payload: AlertPayload,
    channels: list[str] | None = None,
) -> dict[str, str]:
    """Route *payload* to the specified *channels*.

    When *channels* is ``None``, sends only to ``payload.channel``.

    Returns a dict mapping each channel name to its message id (or '' on
    failure).
    """
    if channels is None:
        channels = [payload.channel]

    results: dict[str, str] = {}

    for channel in channels:
        if channel == "telegram":
            from connectors.comms.telegram.sender import send_alert as tg_send

            msg_id = await tg_send(payload)
            results[channel] = msg_id
        elif channel == "discord":
            from connectors.comms.discord.sender import send_alert as dc_send

            msg_id = await dc_send(payload)
            results[channel] = msg_id
        elif channel == "slack":
            from connectors.comms.slack.sender import send_alert as sl_send

            msg_id = await sl_send(payload)
            results[channel] = msg_id
        elif channel == "sms":
            from connectors.comms.twilio.sender import send_alert as sms_send

            results[channel] = await sms_send(payload)
        elif channel == "email":
            from connectors.comms.email.sender import send_alert as email_send

            results[channel] = await email_send(payload)
        else:
            logger.warning("Unknown channel: %s — skipping", channel)
            results[channel] = ""

    return results


async def dispatch_alert(
    opportunity: dict,
    rule: "Rule",
    reason: str,  # noqa: ARG001 — reserved for future use
) -> AlertMsg:
    """Create an ``AlertMsg`` for *opportunity* matching *rule*.

    Returns the constructed ``AlertMsg`` and logs a stub publish to the
    ``alerts.outbound`` bus topic.  Real bus publishing requires EP-004.
    """
    opportunity_id = opportunity.get("id", "")
    net_edge = str(opportunity.get("net_edge", "0"))
    confidence = float(opportunity.get("confidence", 0.0))

    # ── Determine action ───────────────────────────────────────────
    action = opportunity.get("action", "simulate")
    if action not in ("simulate", "execute", "ignore"):
        action = "simulate"

    inline_actions: list[str] = ["simulate", "execute", "ignore"]

    # ── Build summary ──────────────────────────────────────────────
    summary = (
        f"Alert: {rule.name} — {opportunity_id} "
        f"(net_edge={net_edge}, confidence={confidence})"
    )

    channel = rule.channels[0] if rule.channels else "ops"

    alert_msg = AlertMsg(
        schema="aether.alert.v1",
        trace_id=opportunity.get("trace_id", ""),
        ts=now_iso(),
        payload=AlertPayload(
            alert_id=str(ulid.new()),
            rule_name=rule.name,
            opportunity_id=opportunity_id,
            channel=channel,
            summary=summary,
            net_edge=net_edge,
            confidence=confidence,
            action=action,
            inline_actions=inline_actions,
        ),
    )

    # ── Persist to alert_history ───────────────────────────────────
    # Non-fatal: alert delivery continues even if persistence fails.
    try:
        inserted = await record_alert(alert_msg.payload)
        if not inserted:
            alert_msg.payload.status = "duplicate"
    except Exception:
        logger.exception(
            "Failed to record alert %s in history (non-fatal)",
            alert_msg.payload.alert_id,
        )

    # Stub: publish to alerts.outbound bus topic (EP-004 bus required)
    logger.info(
        "dispatch_alert: alert_id=%s rule=%s opp=%s",
        alert_msg.payload.alert_id,
        rule.name,
        opportunity_id,
    )

    return alert_msg


async def deliver_alert(
    opportunity: dict,
    rule: "Rule",
    reason: str,
    publish: Callable[[str, dict], Awaitable[None]] | None = None,
) -> list[AlertMsg]:
    """Persist, publish, and deliver one independently tracked alert per channel."""
    delivered: list[AlertMsg] = []
    channels = rule.channels or ["ops"]
    for channel in channels:
        channel_rule = type(rule)(**{**rule.__dict__, "channels": [channel]})
        msg = await dispatch_alert(opportunity, channel_rule, reason)
        if msg.payload.status == "duplicate":
            delivered.append(msg)
            continue
        if publish is not None:
            await publish("alerts.outbound", msg.model_dump(by_alias=True))
        result = await dispatch_to_channel(msg.payload, [channel])
        message_id = result.get(channel, "")
        status = "sent" if message_id else "failed"
        await update_delivery(
            msg.payload.alert_id,
            status=status,
            message_id=message_id or None,
            last_error=None
            if message_id
            else "channel delivery returned no message id",
        )
        msg.payload.status = status
        msg.payload.message_id = message_id or None
        msg.payload.attempts = 1
        delivered.append(msg)
    return delivered


async def process_action_callback(
    action: str,
    opportunity_id: str,
    operator_id: str,
    operator_tier: int,
) -> dict:
    """Process an inline action callback from an alert channel.

    Validates the action against the operator's tier permissions and
    returns the result.  This is the entry point for channel callbacks
    (telegram button, discord slash command, etc.).  Real callback
    routing requires EP-004.

    Returns the same dict as ``handle_action``.
    """
    parsed: InlineAction | str
    try:
        parsed = InlineAction(action)
    except ValueError:
        parsed = action  # will be rejected by handle_action

    result = await handle_action(
        action=parsed,
        opportunity_id=opportunity_id,
        operator_id=operator_id,
        operator_tier=operator_tier,
    )
    status = result.get("status")
    if status == "authorized":
        if _action_effects is None:
            return {
                **result,
                "status": "failed_precondition",
                "reason": "action effects adapter is not configured",
            }
        try:
            if parsed == InlineAction.IGNORE:
                effect = await _action_effects.ignore(opportunity_id, operator_id)
            elif parsed == InlineAction.SIMULATE:
                effect = await _action_effects.simulate(opportunity_id, operator_id)
            else:
                effect = await _action_effects.execute_paper(
                    opportunity_id, operator_id
                )
        except ActionEffectError:
            logger.exception("authoritative action effect failed")
            return {
                **result,
                "status": "failed_precondition",
                "reason": "authoritative action effect did not complete",
            }
        result = {**result, "status": "completed", "effect": effect}
    logger.info(
        "process_action_callback: action=%s opp=%s op=%s status=%s",
        action,
        opportunity_id,
        operator_id,
        result.get("status"),
    )
    return result
