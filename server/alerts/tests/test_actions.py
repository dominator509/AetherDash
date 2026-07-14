"""Tests for the inline action handler (SPEC-005 tier matrix)."""

import pytest

from server.alerts.actions import InlineAction, handle_action


class TestTier1:
    """Tier 1 — read-only: can only IGNORE."""

    @pytest.mark.asyncio
    async def test_tier_1_cannot_simulate(self) -> None:
        """Tier 1 gets permission_denied for simulate."""
        result = await handle_action(
            action=InlineAction.SIMULATE,
            opportunity_id="opp-001",
            operator_id="op-1",
            operator_tier=1,
        )
        assert result["status"] == "permission_denied"
        assert result["requires_confirm"] is False
        assert "Tier 1" in result["reason"]

    @pytest.mark.asyncio
    async def test_tier_1_cannot_execute(self) -> None:
        """Tier 1 gets permission_denied for execute."""
        result = await handle_action(
            action=InlineAction.EXECUTE,
            opportunity_id="opp-001",
            operator_id="op-1",
            operator_tier=1,
        )
        assert result["status"] == "permission_denied"
        assert result["requires_confirm"] is False

    @pytest.mark.asyncio
    async def test_tier_1_can_ignore(self) -> None:
        """Tier 1 can ignore."""
        result = await handle_action(
            action=InlineAction.IGNORE,
            opportunity_id="opp-001",
            operator_id="op-1",
            operator_tier=1,
        )
        assert result["status"] == "ignored"
        assert result["requires_confirm"] is False


class TestTier2:
    """Tier 2 — draft-only: can SIMULATE or IGNORE."""

    @pytest.mark.asyncio
    async def test_tier_2_can_simulate(self) -> None:
        """Tier 2 can simulate."""
        result = await handle_action(
            action=InlineAction.SIMULATE,
            opportunity_id="opp-002",
            operator_id="op-2",
            operator_tier=2,
        )
        assert result["status"] == "simulated"
        assert result["requires_confirm"] is False

    @pytest.mark.asyncio
    async def test_tier_2_can_ignore(self) -> None:
        """Tier 2 can ignore."""
        result = await handle_action(
            action=InlineAction.IGNORE,
            opportunity_id="opp-002",
            operator_id="op-2",
            operator_tier=2,
        )
        assert result["status"] == "ignored"

    @pytest.mark.asyncio
    async def test_tier_2_cannot_execute(self) -> None:
        """Tier 2 gets permission_denied for execute."""
        result = await handle_action(
            action=InlineAction.EXECUTE,
            opportunity_id="opp-002",
            operator_id="op-2",
            operator_tier=2,
        )
        assert result["status"] == "permission_denied"
        assert result["requires_confirm"] is False


class TestTier3:
    """Tier 3 — confirm-every / paper-only."""

    @pytest.mark.asyncio
    async def test_tier_3_can_simulate(self) -> None:
        """Tier 3 can simulate."""
        result = await handle_action(
            action=InlineAction.SIMULATE,
            opportunity_id="opp-003",
            operator_id="op-3",
            operator_tier=3,
        )
        assert result["status"] == "simulated"

    @pytest.mark.asyncio
    async def test_tier_3_can_ignore(self) -> None:
        """Tier 3 can ignore."""
        result = await handle_action(
            action=InlineAction.IGNORE,
            opportunity_id="opp-003",
            operator_id="op-3",
            operator_tier=3,
        )
        assert result["status"] == "ignored"

    @pytest.mark.asyncio
    async def test_tier_3_can_execute_requires_confirm(self) -> None:
        """Tier 3 execute returns requires_confirm (paper only until EP-305)."""
        result = await handle_action(
            action=InlineAction.EXECUTE,
            opportunity_id="opp-003",
            operator_id="op-3",
            operator_tier=3,
        )
        assert result["status"] == "requires_confirm"
        assert result["requires_confirm"] is True


class TestTier4:
    """Tier 4 — bounded: execute with confirm required."""

    @pytest.mark.asyncio
    async def test_tier_4_can_simulate(self) -> None:
        """Tier 4 can simulate."""
        result = await handle_action(
            action=InlineAction.SIMULATE,
            opportunity_id="opp-004",
            operator_id="op-4",
            operator_tier=4,
        )
        assert result["status"] == "simulated"

    @pytest.mark.asyncio
    async def test_tier_4_execute_requires_confirm(self) -> None:
        """Tier 4 execute returns requires_confirm."""
        result = await handle_action(
            action=InlineAction.EXECUTE,
            opportunity_id="opp-004",
            operator_id="op-4",
            operator_tier=4,
        )
        assert result["status"] == "requires_confirm"
        assert result["requires_confirm"] is True


class TestTier5:
    """Tier 5 — auto-confirm within caps."""

    @pytest.mark.asyncio
    async def test_tier_5_can_simulate(self) -> None:
        """Tier 5 can simulate."""
        result = await handle_action(
            action=InlineAction.SIMULATE,
            opportunity_id="opp-005",
            operator_id="op-5",
            operator_tier=5,
        )
        assert result["status"] == "simulated"

    @pytest.mark.asyncio
    async def test_tier_5_execute_auto_confirm(self) -> None:
        """Tier 5 execute auto-confirms within caps."""
        result = await handle_action(
            action=InlineAction.EXECUTE,
            opportunity_id="opp-005",
            operator_id="op-5",
            operator_tier=5,
        )
        assert result["status"] == "auto_confirmed"
        assert result["requires_confirm"] is False

    @pytest.mark.asyncio
    async def test_tier_5_can_ignore(self) -> None:
        """Tier 5 can ignore."""
        result = await handle_action(
            action=InlineAction.IGNORE,
            opportunity_id="opp-005",
            operator_id="op-5",
            operator_tier=5,
        )
        assert result["status"] == "ignored"


class TestValidation:
    """Input validation edge cases."""

    @pytest.mark.asyncio
    async def test_unknown_action_rejected(self) -> None:
        """Invalid action string is rejected."""
        result = await handle_action(
            action="fly_away",
            opportunity_id="opp-001",
            operator_id="op-1",
            operator_tier=3,
        )
        assert result["status"] == "invalid_action"
        assert result["requires_confirm"] is False

    @pytest.mark.asyncio
    async def test_missing_opportunity_id_rejected(self) -> None:
        """Empty opportunity_id is rejected."""
        result = await handle_action(
            action=InlineAction.SIMULATE,
            opportunity_id="",
            operator_id="op-1",
            operator_tier=3,
        )
        assert result["status"] == "invalid_request"
        assert "opportunity_id" in result["reason"]

    @pytest.mark.asyncio
    async def test_missing_operator_id_rejected(self) -> None:
        """Empty operator_id is rejected."""
        result = await handle_action(
            action=InlineAction.SIMULATE,
            opportunity_id="opp-001",
            operator_id="",
            operator_tier=3,
        )
        assert result["status"] == "invalid_request"
        assert "operator_id" in result["reason"]

    @pytest.mark.asyncio
    async def test_unknown_tier_defaults_to_empty(self) -> None:
        """Unknown tier defaults to empty permissions (permission_denied)."""
        result = await handle_action(
            action=InlineAction.SIMULATE,
            opportunity_id="opp-001",
            operator_id="op-1",
            operator_tier=99,
        )
        assert result["status"] == "permission_denied"
        assert "Tier 99" in result["reason"]

    @pytest.mark.asyncio
    async def test_string_action_normalised(self) -> None:
        """String 'simulate' is normalised to InlineAction automatically."""
        result = await handle_action(
            action="simulate",
            opportunity_id="opp-001",
            operator_id="op-3",
            operator_tier=3,
        )
        assert result["status"] == "simulated"
