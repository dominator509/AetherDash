"""Tests for the alert dispatch module."""

import pytest

from server.alerts.dispatch import dispatch_alert
from server.alerts.models import AlertMsg, AlertPayload
from server.alerts.rules import Rule


class TestDispatchShape:
    """AlertMsg envelope shape tests."""

    @pytest.mark.asyncio
    async def test_dispatch_creates_alert_msg(
        self,
        sample_opportunity: dict,
        sample_rule: Rule,
    ) -> None:
        """dispatch_alert returns an AlertMsg with correct shape."""
        result = await dispatch_alert(sample_opportunity, sample_rule, "reason")
        assert isinstance(result, AlertMsg)
        assert isinstance(result.payload, AlertPayload)

    @pytest.mark.asyncio
    async def test_alert_msg_has_required_fields(
        self,
        sample_opportunity: dict,
        sample_rule: Rule,
    ) -> None:
        """All SPEC-003 alert fields present."""
        result = await dispatch_alert(sample_opportunity, sample_rule, "reason")

        # Envelope fields
        assert result.schema_ == "aether.alert.v1"
        assert result.trace_id == "trace-001"
        assert isinstance(result.ts, str) and len(result.ts) > 0

        # Payload fields
        payload = result.payload
        assert isinstance(payload.alert_id, str) and len(payload.alert_id) == 26
        assert payload.rule_name == "test_rule"
        assert payload.opportunity_id == "01ARZ3NDEKTSV4RRFFQ69G5FAV"
        assert payload.channel == "telegram"
        assert "Alert:" in payload.summary
        assert payload.net_edge == "0.05"
        assert payload.confidence == 0.85
        assert payload.action == "simulate"
        assert isinstance(payload.inline_actions, list)
        assert payload.inline_actions == ["simulate", "execute", "ignore"]

    @pytest.mark.asyncio
    async def test_alert_msg_schema_alias(
        self,
        sample_opportunity: dict,
        sample_rule: Rule,
    ) -> None:
        """schema_ serializes as 'schema' in JSON."""
        result = await dispatch_alert(sample_opportunity, sample_rule, "reason")
        as_dict = result.model_dump(by_alias=True)
        assert "schema" in as_dict
        assert as_dict["schema"] == "aether.alert.v1"

    @pytest.mark.asyncio
    async def test_dispatch_with_custom_channel(
        self,
        sample_opportunity: dict,
    ) -> None:
        """Alert uses first channel from the rule."""
        rule = Rule(name="ops_rule", channels=["ops", "slack"])
        result = await dispatch_alert(sample_opportunity, rule, "reason")
        assert result.payload.channel == "ops"


class TestDispatchEdgeCases:
    """Edge-case tests for dispatch."""

    @pytest.mark.asyncio
    async def test_missing_fields_default(self) -> None:
        """Missing opportunity fields get sensible defaults."""
        opp: dict = {"id": "test-123"}
        rule = Rule(name="catchall")
        result = await dispatch_alert(opp, rule, "reason")
        assert result.payload.net_edge == "0"
        assert result.payload.confidence == 0.0
        assert result.payload.action == "simulate"
        assert result.payload.channel == "telegram"

    @pytest.mark.asyncio
    async def test_invalid_action_defaults(self) -> None:
        """Invalid action values fall back to 'simulate'."""
        opp = {"id": "test-123", "action": "run_away"}
        rule = Rule(name="catchall")
        result = await dispatch_alert(opp, rule, "reason")
        assert result.payload.action == "simulate"

    @pytest.mark.asyncio
    async def test_empty_channels_fallback(self) -> None:
        """Rule with empty channels falls back to 'ops'."""
        opp = {"id": "test-123"}
        rule = Rule(name="no_channels", channels=[])
        result = await dispatch_alert(opp, rule, "reason")
        assert result.payload.channel == "ops"
