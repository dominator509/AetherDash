"""Tests for the Discord comms sender."""

import os
from unittest.mock import AsyncMock, patch

import httpx
import pytest

from connectors.comms.discord.sender import _build_components, _build_embed, send_alert
from server.alerts.models import AlertPayload


@pytest.fixture
def sample_payload() -> AlertPayload:
    return AlertPayload(
        alert_id="01ARZ3NDEKTSV4RRFFQ69G5FAV",
        rule_name="high_edge_arb",
        opportunity_id="opp_001",
        channel="discord",
        summary="Arbitrage: 5.2% edge on Kalshi-BTC",
        net_edge="0.052",
        confidence=0.85,
        action="simulate",
        inline_actions=["simulate", "ignore"],
    )


class TestDiscordEmbed:
    """Embed formatting."""

    def test_embed_includes_rule_name(self, sample_payload: AlertPayload) -> None:
        embed = _build_embed(sample_payload)
        assert sample_payload.rule_name in embed["title"]

    def test_embed_includes_net_edge(self, sample_payload: AlertPayload) -> None:
        embed = _build_embed(sample_payload)
        fields = {f["name"]: f["value"] for f in embed["fields"]}
        assert fields["Net Edge"] == "0.052"

    def test_embed_includes_confidence(self, sample_payload: AlertPayload) -> None:
        embed = _build_embed(sample_payload)
        fields = {f["name"]: f["value"] for f in embed["fields"]}
        assert fields["Confidence"] == "85%"

    def test_embed_has_color(self, sample_payload: AlertPayload) -> None:
        embed = _build_embed(sample_payload)
        assert embed["color"] == 0x00AAFF

    def test_embed_has_timestamp(self, sample_payload: AlertPayload) -> None:
        embed = _build_embed(sample_payload)
        assert embed["timestamp"] is not None


class TestDiscordComponents:
    """Action button components."""

    def test_components_present(self, sample_payload: AlertPayload) -> None:
        components = _build_components(sample_payload)
        assert len(components) == 1  # one ActionRow
        assert components[0]["type"] == 1  # ActionRow

    def test_action_buttons_present(self, sample_payload: AlertPayload) -> None:
        components = _build_components(sample_payload)
        buttons = components[0]["components"]
        assert len(buttons) == 2

        texts = [b["label"] for b in buttons]
        assert "Simulate" in texts
        assert "Ignore" in texts

    def test_button_custom_ids(self, sample_payload: AlertPayload) -> None:
        components = _build_components(sample_payload)
        buttons = components[0]["components"]
        custom_ids = [b["custom_id"] for b in buttons]
        assert "simulate|opp_001" in custom_ids
        assert "ignore|opp_001" in custom_ids

    def test_button_styles(self, sample_payload: AlertPayload) -> None:
        components = _build_components(sample_payload)
        buttons = {b["label"]: b["style"] for b in components[0]["components"]}
        assert buttons["Simulate"] == 1  # Primary
        assert buttons["Ignore"] == 4  # Danger


class TestDiscordSendAlert:
    """send_alert API interaction."""

    @pytest.mark.asyncio
    async def test_send_alert_sends_correct_payload(
        self,
        sample_payload: AlertPayload,
    ) -> None:
        """Verify the correct Discord webhook payload is sent."""
        os.environ["AETHER_DISCORD_WEBHOOK_URL"] = (
            "https://discord.com/api/webhooks/test"
        )

        mock_response = AsyncMock(spec=httpx.Response)
        mock_response.status_code = 204
        mock_response.raise_for_status = lambda: None

        async def fake_post(*args, **kwargs):  # noqa: ARG001
            return mock_response

        with patch("httpx.AsyncClient.post", side_effect=fake_post) as mock_post:
            result = await send_alert(sample_payload)
            assert result == "sent"

            mock_post.assert_called_once()
            _, call_kwargs = mock_post.call_args
            body = call_kwargs["json"]
            assert "embeds" in body
            assert "components" in body
            assert len(body["embeds"]) == 1
            assert sample_payload.rule_name in body["embeds"][0]["title"]

        del os.environ["AETHER_DISCORD_WEBHOOK_URL"]

    @pytest.mark.asyncio
    async def test_send_alert_empty_on_missing_config(
        self,
        sample_payload: AlertPayload,
    ) -> None:
        """Returns '' when webhook URL is missing."""
        os.environ.pop("AETHER_DISCORD_WEBHOOK_URL", None)
        result = await send_alert(sample_payload)
        assert result == ""

    @pytest.mark.asyncio
    async def test_send_alert_handles_http_error(
        self,
        sample_payload: AlertPayload,
    ) -> None:
        """Returns '' on HTTP error."""
        os.environ["AETHER_DISCORD_WEBHOOK_URL"] = (
            "https://discord.com/api/webhooks/test"
        )

        async def fake_post(*args, **kwargs):  # noqa: ARG001
            raise httpx.HTTPStatusError(
                "403 Forbidden",
                request=AsyncMock(spec=httpx.Request),
                response=AsyncMock(spec=httpx.Response),
            )

        with patch("httpx.AsyncClient.post", side_effect=fake_post):
            result = await send_alert(sample_payload)
            assert result == ""

        del os.environ["AETHER_DISCORD_WEBHOOK_URL"]
