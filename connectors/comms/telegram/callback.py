"""Telegram callback handler — parse inline button callbacks and map to operator."""

import logging
import os
from typing import Any

logger = logging.getLogger(__name__)

# Default operator mapping: telegram_user_id -> operator_identity
_DEFAULT_OPERATOR_MAP: dict[str, str] = {}


def _load_operator_map() -> dict[str, str]:
    """Load operator mapping from AETHER_TELEGRAM_OPERATOR_MAP env var.

    Format: comma-separated tg_user_id:operator_id pairs.
    Example: "12345:ops_alice,67890:ops_bob"
    """
    mapping_str = os.environ.get("AETHER_TELEGRAM_OPERATOR_MAP", "")
    if not mapping_str:
        return dict(_DEFAULT_OPERATOR_MAP)
    mapping: dict[str, str] = {}
    for entry in mapping_str.split(","):
        entry = entry.strip()
        if ":" in entry:
            tg_id, op_id = entry.split(":", 1)
            mapping[tg_id.strip()] = op_id.strip()
    return mapping


async def handle_callback(callback_data: dict[str, Any]) -> dict[str, str]:
    """Parse Telegram callback, authenticate operator, dispatch action.

    ``callback_data`` format::

        {"callback_query": {"data": "simulate|opp_123", "from": {"id": "12345"}}}

    Returns an action dict with keys ``action``, ``opportunity_id``,
    ``operator_id``, ``tier``.

    Raises ``ValueError`` on unknown user or malformed data.
    """
    try:
        callback_query = callback_data["callback_query"]
        raw_data = callback_query["data"]
        from_id = str(callback_query["from"]["id"])
    except (KeyError, TypeError) as exc:
        raise ValueError(f"Invalid callback data structure: {exc}") from exc

    # Map Telegram user to operator
    op_map = _load_operator_map()
    operator_id = op_map.get(from_id)
    if not operator_id:
        logger.warning("Unknown Telegram user: %s", from_id)
        raise ValueError(f"Unknown Telegram user: {from_id}")

    # Parse action and opportunity_id
    try:
        action, opportunity_id = raw_data.split("|", 1)
    except ValueError as exc:
        raise ValueError(f"Invalid callback data format: {raw_data}") from exc

    if action not in ("simulate", "execute", "ignore"):
        raise ValueError(f"Unknown action: {action}")

    logger.info(
        "Telegram callback: action=%s opp=%s operator=%s",
        action,
        opportunity_id,
        operator_id,
    )

    return {
        "action": action,
        "opportunity_id": opportunity_id,
        "operator_id": operator_id,
        "tier": "operator",
    }
