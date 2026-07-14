"""Inline action enforcement for AETHER Alerts -- SPEC-005 tier matrix."""

from enum import StrEnum


class InlineAction(StrEnum):
    """Actions an operator can take on an alert callback."""

    SIMULATE = "simulate"
    EXECUTE = "execute"
    IGNORE = "ignore"


# ── Tier matrix ─────────────────────────────────────────────────────────
# Per SPEC-005:
#   tier 1: read-only (only IGNORE)
#   tier 2: draft-only (SIMULATE or IGNORE)
#   tier 3: confirm-every / paper-only until EP-305
#   tier 4: bounded (confirm required)
#   tier 5: auto-confirm within caps

TIER_PERMISSIONS: dict[int, list[InlineAction]] = {
    1: [InlineAction.IGNORE],
    2: [InlineAction.SIMULATE, InlineAction.IGNORE],
    3: [
        InlineAction.SIMULATE,
        InlineAction.IGNORE,
        InlineAction.EXECUTE,
    ],
    4: [
        InlineAction.SIMULATE,
        InlineAction.IGNORE,
        InlineAction.EXECUTE,
    ],
    5: [
        InlineAction.SIMULATE,
        InlineAction.IGNORE,
        InlineAction.EXECUTE,
    ],
}


async def handle_action(
    action: InlineAction | str,
    opportunity_id: str,
    operator_id: str,
    operator_tier: int,
) -> dict:
    """Handle an inline action from an alert callback.

    Tier enforcement per SPEC-005:

    * tier 1 — can only IGNORE
    * tier 2 — can SIMULATE or IGNORE
    * tier 3+ — can SIMULATE, IGNORE, EXECUTE (paper only until EP-305)
    * tier 4+ — EXECUTE with confirm required
    * tier 5  — auto-confirm within caps

    Returns
    -------
    dict
        Keys: ``action``, ``opportunity_id``, ``status``,
        ``requires_confirm``, ``reason``.
    """
    # ── Normalize string to enum ────────────────────────────────────
    if isinstance(action, str):
        try:
            action = InlineAction(action)
        except ValueError:
            return {
                "action": action,
                "opportunity_id": opportunity_id,
                "status": "invalid_action",
                "requires_confirm": False,
                "reason": f"Unknown action: {action}",
            }

    # ── Validate inputs ─────────────────────────────────────────────
    if not opportunity_id or not opportunity_id.strip():
        return {
            "action": action.value,
            "opportunity_id": opportunity_id,
            "status": "invalid_request",
            "requires_confirm": False,
            "reason": "opportunity_id is required",
        }

    if not operator_id or not operator_id.strip():
        return {
            "action": action.value,
            "opportunity_id": opportunity_id,
            "status": "invalid_request",
            "requires_confirm": False,
            "reason": "operator_id is required",
        }

    # ── Check permission against tier matrix ────────────────────────
    allowed = TIER_PERMISSIONS.get(operator_tier, [])
    if action not in allowed:
        return {
            "action": action.value,
            "opportunity_id": opportunity_id,
            "status": "permission_denied",
            "requires_confirm": False,
            "reason": (
                f"Tier {operator_tier} does not have permission for {action.value}"
            ),
        }

    # ── Handle action ───────────────────────────────────────────────
    if action == InlineAction.IGNORE:
        return {
            "action": action.value,
            "opportunity_id": opportunity_id,
            "status": "ignored",
            "requires_confirm": False,
            "reason": "Alert suppressed by operator",
        }

    if action == InlineAction.SIMULATE:
        return {
            "action": action.value,
            "opportunity_id": opportunity_id,
            "status": "simulated",
            "requires_confirm": False,
            "reason": "Simulation queued for execution",
        }

    if action == InlineAction.EXECUTE:
        if operator_tier >= 5:
            return {
                "action": action.value,
                "opportunity_id": opportunity_id,
                "status": "auto_confirmed",
                "requires_confirm": False,
                "reason": (f"Execute action auto-confirmed for tier {operator_tier}"),
            }
        # Tiers 3 and 4 require confirmation
        return {
            "action": action.value,
            "opportunity_id": opportunity_id,
            "status": "requires_confirm",
            "requires_confirm": True,
            "reason": (
                f"Execute action requires confirmation for tier {operator_tier}"
            ),
        }

    # Fallback guard
    return {
        "action": action.value,
        "opportunity_id": opportunity_id,
        "status": "error",
        "requires_confirm": False,
        "reason": f"Unhandled action state for {action.value}",
    }
