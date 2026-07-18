"""MCP enforcement adapter for SPEC-005's canonical action/tier policy.

The Rust ``aether-authz`` crate is authoritative for execution services. This
module mirrors the same stable tool scope names at the Python MCP boundary so a
model never sees or invokes a tool outside its current grant.
"""

from __future__ import annotations

import logging
from dataclasses import dataclass
from enum import StrEnum
from typing import Any

from auth import Session

_audit = logging.getLogger("audit.events")


class Verdict(StrEnum):
    allow = "allow"
    deny = "deny"
    confirm_required = "confirm_required"
    step_up_required = "step_up_required"


@dataclass(frozen=True)
class Decision:
    verdict: Verdict
    rule: str


_MUTATING_TOOLS = {
    "orders.draft",
    "alerts.configure",
    "vault.regenerate",
    "orders.submit_paper",
    "swarm.launch",
    "inbox.reprocess",
    "orders.submit",
    "plugins.install_signed",
    "automation.schedule",
}

_STEP_UP_TOOLS = {
    "orders.submit",  # until the router supplies >=30 days live-history evidence
    "plugins.install_signed",
}


def evaluate_tool(
    session: Session,
    tool: dict[str, Any],
    *,
    confirmed: bool = False,
    step_up_satisfied: bool = False,
) -> Decision:
    """Evaluate one MCP tool call against the current, uncached DB grant."""
    name = str(tool["name"])
    minimum_tier = int(tool["tier"])
    if session.tier < minimum_tier:
        return Decision(Verdict.deny, "tier.insufficient")

    scopes = session.scopes or {}
    allowed = scopes.get("allowed")
    if allowed is not None and name not in allowed:
        return Decision(Verdict.deny, "grant.scope_denied")

    if name in _STEP_UP_TOOLS and not step_up_satisfied:
        return Decision(Verdict.step_up_required, "step_up.required")
    if session.tier == 3 and name in _MUTATING_TOOLS and not confirmed:
        return Decision(Verdict.confirm_required, "confirmation.required")
    if session.tier == 4 and name == "orders.submit" and not confirmed:
        return Decision(Verdict.confirm_required, "confirmation.required")
    return Decision(Verdict.allow, "tier.allowed")


def emit_decision(session: Session, tool_name: str, decision: Decision) -> None:
    """Emit metadata only; authorization headers, bodies, and credentials stay out."""
    _audit.info(
        "authorization decision",
        extra={
            "actor_id": session.actor_id,
            "actor_kind": session.origin_kind,
            "action": tool_name,
            "grant_id": session.grant_id,
            "verdict": decision.verdict.value,
            "deciding_rule": decision.rule,
            "enforcement_point": "mcp",
        },
    )
