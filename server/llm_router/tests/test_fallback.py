"""Unit tests for the LLM Router fallback module."""

from __future__ import annotations

from unittest.mock import AsyncMock, patch

import pytest

import server.llm_router.fallback as fallback_module
from server.llm_router.fallback import (
    _classify_error,
    complete_with_fallback,
    get_fallback_metrics_text,
)

# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------


@pytest.fixture(autouse=True)
def reset_fallback_counters():
    """Reset in-memory fallback counters before each test."""
    fallback_module._fallback_total = 0
    fallback_module._fallback_events.clear()
    yield


# ---------------------------------------------------------------------------
# Error classification
# ---------------------------------------------------------------------------


class TestClassifyError:
    """_classify_error categorises error messages correctly."""

    def test_timeout_is_transient(self):
        assert _classify_error("Request timed out after 30s") == "transient"

    def test_rate_limit_is_transient(self):
        assert _classify_error("Rate limit exceeded: 429") == "transient"

    def test_5xx_is_transient(self):
        assert _classify_error("502 Bad Gateway") == "transient"
        assert _classify_error("503 Service Unavailable") == "transient"

    def test_unauthorized_is_fatal(self):
        assert _classify_error("401 Unauthorized") == "fatal"
        assert _classify_error("403 Forbidden") == "fatal"

    def test_api_key_error_is_fatal(self):
        assert _classify_error("Invalid API key provided") == "fatal"
        assert _classify_error("Authentication failed") == "fatal"

    def test_context_length_is_context(self):
        assert _classify_error("Context length exceeded") == "context"
        assert _classify_error("This model's maximum context length is") == "context"

    def test_default_to_transient(self):
        """Unrecognised errors default to transient (safe fallback)."""
        assert _classify_error("Some unknown error occurred") == "transient"


# ---------------------------------------------------------------------------
# complete_with_fallback
# ---------------------------------------------------------------------------


def _make_success_response(
    text: str = "ok",
    provider: str = "anthropic",
    model: str = "claude-sonnet-5",
) -> dict:
    """Build a fake success response dict."""
    return {
        "text": text,
        "usage": {"prompt_tokens": 10, "completion_tokens": 20},
        "model": model,
        "provider": provider,
        "cache_hit": False,
        "cost_usd": 0.001,
        "trace_id": "abc",
    }


def _make_error_response(
    text: str = "LiteLLM call failed: timeout",
    provider: str = "anthropic",
    model: str = "claude-sonnet-5",
) -> dict:
    """Build a fake error response dict (as produced by router.complete)."""
    return {
        "text": text,
        "usage": {"prompt_tokens": 0, "completion_tokens": 0},
        "model": model,
        "provider": provider,
        "cache_hit": False,
        "cost_usd": 0.0,
        "trace_id": "abc",
    }


class TestCompleteWithFallback:
    """complete_with_fallback routes through the fallback chain."""

    # ------------------------------------------------------------------
    # Default -> cheap on timeout
    # ------------------------------------------------------------------

    @pytest.mark.asyncio
    @patch("server.llm_router.fallback.complete", new_callable=AsyncMock)
    async def test_default_falls_to_cheap_on_timeout(self, mock_complete):
        """When the default provider times out, fallback to cheap works."""
        mock_complete.side_effect = [
            _make_error_response(provider="anthropic"),  # default fails
            _make_success_response(provider="deepseek"),  # cheap succeeds
        ]

        result = await complete_with_fallback(
            "chat",
            messages=[{"role": "user", "content": "hi"}],
            model_policy="default",
        )

        assert result["text"] == "ok"
        assert result["provider"] == "deepseek"
        assert result["fallback_chain"] == ["default", "cheap"]
        # Fallback counter should have incremented
        assert fallback_module._fallback_total == 1

    # ------------------------------------------------------------------
    # Default -> cheap -> local on double failure
    # ------------------------------------------------------------------

    @pytest.mark.asyncio
    @patch("server.llm_router.fallback.complete", new_callable=AsyncMock)
    async def test_double_fallback_all_the_way(self, mock_complete):
        """Default fails -> cheap fails -> local succeeds."""
        mock_complete.side_effect = [
            _make_error_response(provider="anthropic"),
            _make_error_response(provider="deepseek"),
            _make_success_response(provider="ollama"),
        ]

        result = await complete_with_fallback(
            "chat",
            messages=[{"role": "user", "content": "hi"}],
            model_policy="default",
        )

        assert result["text"] == "ok"
        assert result["provider"] == "ollama"
        assert result["fallback_chain"] == ["default", "cheap", "local"]
        # Two fallback events: anthropic -> deepseek, deepseek -> ollama
        assert fallback_module._fallback_total == 2

    # ------------------------------------------------------------------
    # Auth error -> no fallback
    # ------------------------------------------------------------------

    @pytest.mark.asyncio
    @patch("server.llm_router.fallback.complete", new_callable=AsyncMock)
    async def test_auth_error_halts_fallback(self, mock_complete):
        """An auth error does NOT trigger fallback (fatal config error)."""
        mock_complete.return_value = _make_error_response(
            text="LiteLLM call failed: Invalid API key provided",
            provider="anthropic",
        )

        result = await complete_with_fallback(
            "chat",
            messages=[{"role": "user", "content": "hi"}],
            model_policy="default",
        )

        assert "Non-retryable error" in result["error"]
        assert fallback_module._fallback_total == 0

    @pytest.mark.asyncio
    @patch("server.llm_router.fallback.complete", new_callable=AsyncMock)
    async def test_context_error_halts_fallback(self, mock_complete):
        """A context-length error does NOT trigger fallback."""
        mock_complete.return_value = _make_error_response(
            text="LiteLLM call failed: Context length exceeded",
            provider="anthropic",
        )

        result = await complete_with_fallback(
            "chat",
            messages=[{"role": "user", "content": "big input"}],
            model_policy="default",
        )

        assert "Non-retryable error" in result["error"]
        assert fallback_module._fallback_total == 0

    # ------------------------------------------------------------------
    # All exhausted -> error dict
    # ------------------------------------------------------------------

    @pytest.mark.asyncio
    @patch("server.llm_router.fallback.complete", new_callable=AsyncMock)
    async def test_all_exhausted_returns_error_dict(self, mock_complete):
        """When all providers fail, return an 'all providers exhausted' error."""
        mock_complete.side_effect = [
            _make_error_response(provider="anthropic"),
            _make_error_response(provider="deepseek"),
            _make_error_response(provider="ollama"),
        ]

        result = await complete_with_fallback(
            "chat",
            messages=[{"role": "user", "content": "hi"}],
            model_policy="default",
        )

        assert result["error"] == "all providers exhausted"
        assert "All providers exhausted" in result["text"]
        assert result["fallback_chain"] == ["default", "cheap", "local"]
        assert fallback_module._fallback_total == 2

    @pytest.mark.asyncio
    @patch("server.llm_router.fallback.complete", new_callable=AsyncMock)
    async def test_all_exhausted_local(self, mock_complete):
        """When only local is in the chain and it fails, return exhausted."""
        mock_complete.return_value = _make_error_response(provider="ollama")

        result = await complete_with_fallback(
            "chat",
            messages=[{"role": "user", "content": "hi"}],
            model_policy="local",
        )

        assert result["error"] == "all providers exhausted"
        assert (
            fallback_module._fallback_total == 0
        )  # no fallback events (only one in chain)

    # ------------------------------------------------------------------
    # Immediate success (no fallback)
    # ------------------------------------------------------------------

    @pytest.mark.asyncio
    @patch("server.llm_router.fallback.complete", new_callable=AsyncMock)
    async def test_immediate_success_no_fallback(self, mock_complete):
        """When the first provider succeeds, no fallback occurs."""
        mock_complete.return_value = _make_success_response(provider="anthropic")

        result = await complete_with_fallback(
            "chat",
            messages=[{"role": "user", "content": "hi"}],
            model_policy="default",
        )

        assert result["text"] == "ok"
        assert result["provider"] == "anthropic"
        assert fallback_module._fallback_total == 0

    # ------------------------------------------------------------------
    # Unknown purpose raises ValueError
    # ------------------------------------------------------------------

    @pytest.mark.asyncio
    async def test_unknown_purpose_raises_value_error(self):
        """An unknown purpose propagates as ValueError."""
        with pytest.raises(ValueError, match="Unknown purpose"):
            await complete_with_fallback(
                "invalid-purpose",
                messages=[{"role": "user", "content": "test"}],
            )


# ---------------------------------------------------------------------------
# Fallback metrics
# ---------------------------------------------------------------------------


class TestFallbackMetrics:
    """Fallback metrics increment correctly."""

    @pytest.mark.asyncio
    @patch("server.llm_router.fallback.complete", new_callable=AsyncMock)
    async def test_fallback_counter_increments(self, mock_complete):
        """Each fallback event increments the counter."""
        mock_complete.side_effect = [
            _make_error_response(provider="anthropic"),
            _make_success_response(provider="deepseek"),
        ]

        await complete_with_fallback(
            "chat",
            messages=[{"role": "user", "content": "hi"}],
            model_policy="default",
        )

        assert fallback_module._fallback_total == 1

    @pytest.mark.asyncio
    @patch("server.llm_router.fallback.complete", new_callable=AsyncMock)
    async def test_multiple_fallbacks_increment_multiple(self, mock_complete):
        """Multiple fallback events each increment the counter."""
        mock_complete.side_effect = [
            _make_error_response(provider="anthropic"),
            _make_error_response(provider="deepseek"),
            _make_success_response(provider="ollama"),
        ]

        await complete_with_fallback(
            "chat",
            messages=[{"role": "user", "content": "hi"}],
            model_policy="default",
        )

        assert fallback_module._fallback_total == 2

    def test_get_fallback_metrics_text_format(self):
        """get_fallback_metrics_text returns valid Prometheus text."""
        # Manually seed
        fallback_module._fallback_events.append({"from": "anthropic", "to": "deepseek"})
        fallback_module._fallback_events.append({"from": "deepseek", "to": "ollama"})
        fallback_module._fallback_total = 2

        text = get_fallback_metrics_text()
        assert "# HELP" in text
        assert "# TYPE" in text
        assert "aether_llm_fallback_total" in text
        assert 'from_provider="anthropic"' in text
        assert 'to_provider="ollama"' in text

    def test_get_fallback_metrics_text_empty(self):
        """With no fallback events, output is minimal."""
        text = get_fallback_metrics_text()
        assert "# HELP" in text
        assert "# TYPE" in text
        # No data lines
        data_lines = [ln for ln in text.split("\n") if ln and not ln.startswith("#")]
        assert len(data_lines) == 0
