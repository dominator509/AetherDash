"""EP-203 migrated Postgres + Redpanda integration acceptance."""

from __future__ import annotations

import asyncio
import json
import os
from contextlib import suppress
from unittest.mock import AsyncMock, patch

import pytest
import ulid
from aiokafka import AIOKafkaConsumer

from server.alerts import history
from server.alerts.bus import AlertBus
from server.alerts.dispatch import deliver_alert
from server.alerts.rules import Rule, _reset_state, evaluate


async def _process_one(bus: AlertBus, rule: Rule) -> None:
    async for opportunity in bus.opportunities():
        matches = await evaluate(opportunity, [rule])
        for matched_rule, reason in matches:
            await deliver_alert(opportunity, matched_rule, reason, bus.publish)
        return


@pytest.mark.integration
@pytest.mark.asyncio
async def test_scripted_opportunity_persists_delivers_and_emits_outbound() -> None:
    if os.environ.get("AETHER_INTEGRATION_TEST") != "1":
        pytest.skip("set AETHER_INTEGRATION_TEST=1 and use a migrated Postgres")

    bootstrap = os.environ.get("AETHER_KAFKA_BOOTSTRAP", "localhost:9092")
    suffix = str(ulid.new()).lower()
    opportunity_id = str(ulid.new())
    rule = Rule(
        name=f"ep203_integration_{suffix}",
        min_net_edge="0.03",
        channels=["telegram"],
        rate_limit_per_minute=1,
    )
    bus = AlertBus(
        bootstrap=bootstrap,
        group_id=f"svc.alerts.integration.{suffix}",
        auto_offset_reset="latest",
    )
    outbound = AIOKafkaConsumer(
        "alerts.outbound",
        bootstrap_servers=bootstrap,
        group_id=f"svc.alerts.integration.outbound.{suffix}",
        auto_offset_reset="latest",
        enable_auto_commit=False,
        value_deserializer=lambda value: json.loads(value.decode()),
    )
    processor: asyncio.Task[None] | None = None
    alert_id: str | None = None
    _reset_state()
    await history.close_pool()
    await bus.start()
    await outbound.start()
    try:
        # Let both unique consumer groups receive their assignments before the
        # scripted source event is published at the latest offset.
        await asyncio.sleep(1)
        processor = asyncio.create_task(_process_one(bus, rule))
        with patch(
            "server.alerts.dispatch.dispatch_to_channel",
            new=AsyncMock(return_value={"telegram": f"stub-{suffix}"}),
        ):
            await bus.publish(
                "opps.detected",
                {
                    "schema": "aether.opportunity.v1",
                    "trace_id": str(ulid.new()),
                    "payload": {
                        "id": opportunity_id,
                        "kind": "arbitrage",
                        "net_edge": "0.05",
                        "confidence": 0.91,
                        "trace_id": str(ulid.new()),
                    },
                },
            )
            await asyncio.wait_for(processor, timeout=15)
            message = await asyncio.wait_for(outbound.getone(), timeout=15)

        envelope = message.value
        assert envelope["schema"] == "aether.alert.v1"
        assert envelope["payload"]["opportunity_id"] == opportunity_id
        assert envelope["payload"]["rule_name"] == rule.name
        alert_id = envelope["payload"]["alert_id"]

        pool = await history.get_pool()
        async with pool.acquire() as conn:
            row = await conn.fetchrow(
                """
                SELECT status, message_id, attempts
                FROM alert_history
                WHERE id = $1
                """,
                alert_id,
            )
        assert row is not None
        assert row["status"] == "sent"
        assert row["message_id"] == f"stub-{suffix}"
        assert row["attempts"] == 1
    finally:
        if processor is not None and not processor.done():
            processor.cancel()
            with suppress(asyncio.CancelledError):
                await processor
        if alert_id is not None:
            pool = await history.get_pool()
            async with pool.acquire() as conn:
                await conn.execute("DELETE FROM alert_history WHERE id = $1", alert_id)
        await outbound.stop()
        await bus.stop()
        await history.close_pool()
        _reset_state()
