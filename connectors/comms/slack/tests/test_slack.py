"""Tests for the Slack comms sender."""

import os
from unittest.mock import AsyncMock, patch

import httpx
import pytest

from connectors.comms.slack.sender import _build_blocks, send_alert
from server.alerts.models import AlertPayload


@pytest.fixture
def sample_payload() -> AlertPayload:
    return AlertPayload(
        alert_id="01ARZ3NDEKTSV4RRFFQ69G5FAV",
        rule_name="high_edge_arb",
        opportunity_id="opp_001",
        channel="slack",
        summary="Arbitrage: 5.2% edge on Kalshi-BTC",
        net_edge="0.052",
        confidence=0.85,
        action="simulate",
        inline_actions=["simulate", "ignore"],
    )


class TestSlackBlocks:
    """Block Kit block formatting."""

    def test_blocks_include_header(self, sample_payload: AlertPayload) -> None:
        blocks = _build_blocks(sample_payload)
        header = blocks[0]
        assert header["type"] == "header"
        assert sample_payload.rule_name in header["text"]["text"]

    def test_blocks_include_section(self, sample_payload: AlertPayload) -> None:
        blocks = _build_blocks(sample_payload)
        section = blocks[1]
        assert section["type"] == "section"
        assert sample_payload.summary in section["text"]["text"]

    def test_section_has_net_edge(self, sample_payload: AlertPayload) -> None:
        blocks = _build_blocks(sample_payload)
        section = blocks[1]
        fields_text = " ".join(f["text"] for f in section["fields"])
        assert sample_payload.net_edge in fields_text

    def test_section_has_confidence(self, sample_payload: AlertPayload) -> None:
        blocks = _build_blocks(sample_payload)
        section = blocks[1]
        fields_text = " ".join(f["text"] for f in section["fields"])
        assert "85%" in fields_text

    def test_blocks_include_divider(self, sample_payload: AlertPayload) -> None:
        blocks = _build_blocks(sample_payload)
        assert any(b["type"] == "divider" for b in blocks)

    def test_blocks_include_actions(self, sample_payload: AlertPayload) -> None:
        blocks = _build_blocks(sample_payload)
        actions_block = [b for b in blocks if b["type"] == "actions"]
        assert len(actions_block) == 1

    def test_action_buttons_present(self, sample_payload: AlertPayload) -> None:
        blocks = _build_blocks(sample_payload)
        actions_block = next(b for b in blocks if b["type"] == "actions")
        buttons = actions_block["elements"]
        assert len(buttons) == 2
        texts = [b["text"]["text"] for b in buttons]
        assert "Simulate" in texts
        assert "Ignore" in texts

    def test_button_values(self, sample_payload: AlertPayload) -> None:
        blocks = _build_blocks(sample_payload)
        actions_block = next(b for b in blocks if b["type"] == "actions")
        values = [b["value"] for b in actions_block["elements"]]
        assert "simulate|opp_001" in values
        assert "ignore|opp_001" in values


class TestSlackSendAlert:
    """send_alert API interaction."""

    @pytest.mark.asyncio
    async def test_send_alert_sends_correct_payload(
        self,
        sample_payload: AlertPayload,
    ) -> None:
        """Verify the correct Slack webhook payload is sent."""
        os.environ["AETHER_SLACK_WEBHOOK_URL"] = "https://hooks.slack.com/services/test"

        mock_response = AsyncMock(spec=httpx.Response)
        mock_response.status_code = 200
        mock_response.raise_for_status = lambda: None

        async def fake_post(*args, **kwargs):  # noqa: ARG001
            return mock_response

        with patch("httpx.AsyncClient.post", side_effect=fake_post) as mock_post:
            result = await send_alert(sample_payload)
            assert result == "sent"

            mock_post.assert_called_once()
            _, call_kwargs = mock_post.call_args
            body = call_kwargs["json"]
            assert "blocks" in body
            assert len(body["blocks"]) > 0
            assert body["blocks"][0]["type"] == "header"

        del os.environ["AETHER_SLACK_WEBHOOK_URL"]

    @pytest.mark.asyncio
    async def test_send_alert_empty_on_missing_config(
        self,
        sample_payload: AlertPayload,
    ) -> None:
        """Returns '' when webhook URL is missing."""
        os.environ.pop("AETHER_SLACK_WEBHOOK_URL", None)
        result = await send_alert(sample_payload)
        assert result == ""

    @pytest.mark.asyncio
    async def test_send_alert_handles_http_error(
        self,
        sample_payload: AlertPayload,
    ) -> None:
        """Returns '' on HTTP error."""
        os.environ["AETHER_SLACK_WEBHOOK_URL"] = "https://hooks.slack.com/services/test"

        async def fake_post(*args, **kwargs):  # noqa: ARG001
            raise httpx.HTTPStatusError(
                "403 Forbidden",
                request=AsyncMock(spec=httpx.Request),
                response=AsyncMock(spec=httpx.Response),
            )

        with patch("httpx.AsyncClient.post", side_effect=fake_post):
            result = await send_alert(sample_payload)
            assert result == ""

        del os.environ["AETHER_SLACK_WEBHOOK_URL"]
