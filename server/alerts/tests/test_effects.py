"""Effect-level tests for callbacks and multi-channel delivery."""

from unittest.mock import AsyncMock, patch

import pytest

from server.alerts.dispatch import (
    configure_action_effects,
    deliver_alert,
    process_action_callback,
)
from server.alerts.rules import Rule


class FakeEffects:
    def __init__(self) -> None:
        self.simulations: list[tuple[str, str]] = []
        self.ignored: list[tuple[str, str]] = []
        self.executions: list[tuple[str, str]] = []

    async def simulate(self, opportunity_id: str, actor_id: str) -> dict:
        self.simulations.append((opportunity_id, actor_id))
        return {"status": "completed"}

    async def ignore(self, opportunity_id: str, actor_id: str) -> dict:
        self.ignored.append((opportunity_id, actor_id))
        return {"status": "completed"}

    async def execute_paper(self, opportunity_id: str, actor_id: str) -> dict:
        self.executions.append((opportunity_id, actor_id))
        return {"status": "completed"}


@pytest.mark.asyncio
async def test_callback_requires_effect_adapter() -> None:
    configure_action_effects(None)
    result = await process_action_callback("simulate", "opp-1", "actor-1", 2)
    assert result["status"] == "failed_precondition"


@pytest.mark.asyncio
async def test_callback_invokes_effect_after_policy_allows() -> None:
    effects = FakeEffects()
    configure_action_effects(effects)
    try:
        result = await process_action_callback("ignore", "opp-1", "actor-1", 1)
    finally:
        configure_action_effects(None)
    assert result["status"] == "completed"
    assert effects.ignored == [("opp-1", "actor-1")]


@pytest.mark.asyncio
async def test_delivery_tracks_every_channel() -> None:
    rule = Rule(name="r", channels=["telegram", "discord", "slack"])
    opportunity = {"id": "opp-1", "trace_id": "trace-1"}
    published: list[tuple[str, dict]] = []

    async def publish(topic: str, envelope: dict) -> None:
        published.append((topic, envelope))

    with (
        patch("server.alerts.dispatch.record_alert", AsyncMock(return_value=True)),
        patch("server.alerts.dispatch.update_delivery", AsyncMock()) as update,
        patch(
            "server.alerts.dispatch.dispatch_to_channel",
            AsyncMock(
                side_effect=lambda payload, channels: {channels[0]: "message-id"}
            ),
        ),
    ):
        results = await deliver_alert(opportunity, rule, "matched", publish)

    assert [item.payload.channel for item in results] == rule.channels
    assert [item[0] for item in published] == ["alerts.outbound"] * 3
    assert update.await_count == 3
