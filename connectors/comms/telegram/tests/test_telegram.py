"""Tests for the Telegram comms sender and callback."""

import os
from unittest.mock import AsyncMock, patch

import httpx
import pytest

from connectors.comms.telegram.callback import handle_callback
from connectors.comms.telegram.sender import (
    _build_keyboard,
    _format_message,
    send_alert,
)
from server.alerts.models import AlertPayload

# ── Fixtures ──────────────────────────────────────────────────────────────


@pytest.fixture
def sample_payload() -> AlertPayload:
    return AlertPayload(
        alert_id="01ARZ3NDEKTSV4RRFFQ69G5FAV",
        rule_name="high_edge_arb",
        opportunity_id="opp_001",
        channel="telegram",
        summary="Arbitrage: 5.2% edge on Kalshi-BTC",
        net_edge="0.052",
        confidence=0.85,
        action="simulate",
        inline_actions=["simulate", "ignore"],
    )


@pytest.fixture
def valid_callback() -> dict:
    return {
        "callback_query": {
            "data": "simulate|opp_001",
            "from": {"id": "12345"},
        },
    }


@pytest.fixture
def unknown_user_callback() -> dict:
    return {
        "callback_query": {
            "data": "simulate|opp_001",
            "from": {"id": "99999"},
        },
    }


# ── Message formatting tests ──────────────────────────────────────────────


class TestTelegramFormatMessage:
    """_format_message output shape."""

    def test_format_includes_rule_name(self, sample_payload: AlertPayload) -> None:
        text = _format_message(sample_payload)
        assert sample_payload.rule_name in text

    def test_format_includes_net_edge(self, sample_payload: AlertPayload) -> None:
        text = _format_message(sample_payload)
        assert sample_payload.net_edge in text

    def test_format_includes_confidence(self, sample_payload: AlertPayload) -> None:
        text = _format_message(sample_payload)
        assert str(sample_payload.confidence) in text

    def test_format_multiline(self, sample_payload: AlertPayload) -> None:
        text = _format_message(sample_payload)
        assert "\n" in text


class TestTelegramBuildKeyboard:
    """Inline keyboard markup."""

    def test_keyboard_has_buttons(self, sample_payload: AlertPayload) -> None:
        kb = _build_keyboard(sample_payload)
        keyboard = kb["inline_keyboard"]
        assert len(keyboard) == 2  # two rows, one button each
        assert keyboard[0][0]["text"] == "\U0001f9ea Simulate"
        assert keyboard[1][0]["text"] == "\U0001f4cb Ignore"

    def test_keyboard_callback_data(self, sample_payload: AlertPayload) -> None:
        kb = _build_keyboard(sample_payload)
        row0 = kb["inline_keyboard"][0][0]
        assert row0["callback_data"] == "simulate|opp_001"

        row1 = kb["inline_keyboard"][1][0]
        assert row1["callback_data"] == "ignore|opp_001"


# ── send_alert tests ─────────────────────────────────────────────────────


class TestTelegramSendAlert:
    """send_alert API interaction."""

    @pytest.mark.asyncio
    async def test_send_alert_correct_api_call(
        self,
        sample_payload: AlertPayload,
    ) -> None:
        """Verify the correct Telegram API call is made."""
        os.environ["AETHER_TELEGRAM_BOT_TOKEN"] = "test_token"
        os.environ["AETHER_TELEGRAM_CHAT_ID"] = "-100123456789"

        mock_response = AsyncMock(spec=httpx.Response)
        mock_response.status_code = 200
        mock_response.json.return_value = {
            "ok": True,
            "result": {"message_id": 42},
        }
        mock_response.raise_for_status = lambda: None

        async def fake_post(*args, **kwargs):  # noqa: ARG001
            return mock_response

        with (
            patch("httpx.AsyncClient.post", side_effect=fake_post) as mock_post,
            patch.dict(os.environ, {}, clear=False),
        ):
            os.environ["AETHER_TELEGRAM_BOT_TOKEN"] = "test_token"
            os.environ["AETHER_TELEGRAM_CHAT_ID"] = "-100123456789"

            msg_id = await send_alert(sample_payload)

            assert msg_id == "42"
            mock_post.assert_called_once()
            _, call_kwargs = mock_post.call_args
            body = call_kwargs["json"]
            assert body["chat_id"] == "-100123456789"
            assert body["parse_mode"] == "Markdown"
            assert "reply_markup" in body
            assert sample_payload.rule_name in body["text"]

        del os.environ["AETHER_TELEGRAM_BOT_TOKEN"]
        del os.environ["AETHER_TELEGRAM_CHAT_ID"]

    @pytest.mark.asyncio
    async def test_send_alert_fallback_on_api_error(
        self,
        sample_payload: AlertPayload,
    ) -> None:
        """Fallback gracefully on API error — returns ''."""
        os.environ["AETHER_TELEGRAM_BOT_TOKEN"] = "test_token"
        os.environ["AETHER_TELEGRAM_CHAT_ID"] = "-100123456789"

        mock_response = AsyncMock(spec=httpx.Response)
        mock_response.status_code = 200
        mock_response.json.return_value = {
            "ok": False,
            "description": "Bad Request: can't parse entities",
        }
        mock_response.raise_for_status = lambda: None

        call_count = 0

        async def fake_post(*args, **kwargs):  # noqa: ARG001
            nonlocal call_count
            call_count += 1
            return mock_response

        with patch("httpx.AsyncClient.post", side_effect=fake_post):
            msg_id = await send_alert(sample_payload)
            assert msg_id == ""  # fallback also returned not-ok
            assert call_count == 2  # original + fallback

        del os.environ["AETHER_TELEGRAM_BOT_TOKEN"]
        del os.environ["AETHER_TELEGRAM_CHAT_ID"]

    @pytest.mark.asyncio
    async def test_send_alert_empty_on_missing_config(
        self,
        sample_payload: AlertPayload,
    ) -> None:
        """Returns '' when env vars are missing."""
        # Ensure env vars are not set
        os.environ.pop("AETHER_TELEGRAM_BOT_TOKEN", None)
        os.environ.pop("AETHER_TELEGRAM_CHAT_ID", None)

        msg_id = await send_alert(sample_payload)
        assert msg_id == ""


# ── Callback tests ────────────────────────────────────────────────────────


class TestTelegramCallback:
    """Callback parsing and operator mapping."""

    @pytest.mark.asyncio
    async def test_callback_parses_action_and_opportunity(
        self,
        valid_callback: dict,
    ) -> None:
        """Callback extracts action and opportunity_id correctly."""
        with patch.dict(
            os.environ,
            {"AETHER_TELEGRAM_OPERATOR_MAP": "12345:ops_alice"},
        ):
            result = await handle_callback(valid_callback)

        assert result["action"] == "simulate"
        assert result["opportunity_id"] == "opp_001"
        assert result["operator_id"] == "ops_alice"
        assert result["tier"] == "operator"

    @pytest.mark.asyncio
    async def test_unknown_user_rejected(
        self,
        unknown_user_callback: dict,
    ) -> None:
        """Unknown Telegram user ID raises ValueError."""
        with patch.dict(
            os.environ,
            {"AETHER_TELEGRAM_OPERATOR_MAP": "12345:ops_alice"},
        ):
            with pytest.raises(ValueError, match="Unknown Telegram user"):
                await handle_callback(unknown_user_callback)

    @pytest.mark.asyncio
    async def test_malformed_callback_data_rejected(self) -> None:
        """Malformed callback data raises ValueError."""
        bad = {"callback_query": {"data": "badformat", "from": {"id": "12345"}}}
        with patch.dict(
            os.environ,
            {"AETHER_TELEGRAM_OPERATOR_MAP": "12345:ops_alice"},
        ):
            with pytest.raises(ValueError, match="Invalid callback data format"):
                await handle_callback(bad)

    @pytest.mark.asyncio
    async def test_missing_callback_query_rejected(self) -> None:
        """Missing callback_query key raises ValueError."""
        with pytest.raises(ValueError, match="Invalid callback data structure"):
            await handle_callback({})
