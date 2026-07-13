"""Tests for LLM Router accounting and metrics."""

from __future__ import annotations

import logging
from unittest.mock import AsyncMock, MagicMock, patch

import pytest

import server.llm_router.metrics as metrics_module
from server.llm_router.accounting import record_call
from server.llm_router.metrics import get_metrics_text, record_metrics

# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------


@pytest.fixture(autouse=True)
def reset_metrics():
    """Reset all in-memory metrics counters before each test."""
    metrics_module._cache_hits = 0
    metrics_module._cache_misses = 0
    metrics_module._calls_by_purpose.clear()
    metrics_module._cost_by_provider_model_purpose.clear()
    yield


# ---------------------------------------------------------------------------
# record_call — ClickHouse accounting
# ---------------------------------------------------------------------------


class TestRecordCall:
    """record_call() writes to ClickHouse via HTTP."""

    async def _call(self, **overrides):
        """Helper: call record_call with sensible defaults."""
        kwargs = dict(
            purpose="chat",
            provider="anthropic",
            model="claude-sonnet-5",
            prompt_tokens=50,
            completion_tokens=100,
            cache_hit=False,
            cost_usd=0.003,
            latency_ms=1200.5,
            trace_id="abc123",
        )
        kwargs.update(overrides)
        await record_call(**kwargs)

    @pytest.mark.asyncio
    async def test_sends_post_to_clickhouse(self):
        """record_call sends an HTTP POST to the ClickHouse URL."""
        mock_post = AsyncMock()
        mock_post.return_value = MagicMock(status_code=200)

        mock_client = MagicMock()
        mock_client.__aenter__.return_value.post = mock_post

        with patch(
            "server.llm_router.accounting.httpx.AsyncClient",
            return_value=mock_client,
        ):
            await self._call()

        assert mock_post.called, "Expected a POST to ClickHouse"
        assert "llm_calls" in str(mock_post.call_args)

    @pytest.mark.asyncio
    async def test_includes_insert_query(self):
        """The POST includes an INSERT query with correct column names."""
        mock_post = AsyncMock()
        mock_post.return_value = MagicMock(status_code=200)

        mock_client = MagicMock()
        mock_client.__aenter__.return_value.post = mock_post

        with patch(
            "server.llm_router.accounting.httpx.AsyncClient",
            return_value=mock_client,
        ):
            await self._call()

        _, kwargs = mock_post.call_args
        assert kwargs["auth"] == ("aether", "aether")
        params = kwargs.get("params", {})
        query = params.get("query", "")
        assert "INSERT INTO" in query
        assert "llm_calls" in query
        assert "prompt_tokens" in query
        assert "completion_tokens" in query
        assert "cached_tokens" in query
        assert "cost_usd" in query
        assert "cache_hit" in query
        assert "latency_ms" in query
        assert "trace_id" in query

    @pytest.mark.asyncio
    async def test_includes_values_with_correct_data(self):
        """The INSERT VALUES clause contains the provider/model/purpose."""
        mock_post = AsyncMock()
        mock_post.return_value = MagicMock(status_code=200)

        mock_client = MagicMock()
        mock_client.__aenter__.return_value.post = mock_post

        with patch(
            "server.llm_router.accounting.httpx.AsyncClient",
            return_value=mock_client,
        ):
            await self._call()

        _, kwargs = mock_post.call_args
        params = kwargs.get("params", {})
        query = params.get("query", "")
        assert "anthropic" in query
        assert "claude-sonnet-5" in query
        assert "chat" in query
        assert "abc123" in query

    @pytest.mark.asyncio
    async def test_cache_hit_sets_cached_tokens(self):
        """A cache-hit call has cached_tokens == prompt_tokens and cost 0."""
        mock_post = AsyncMock()
        mock_post.return_value = MagicMock(status_code=200)

        mock_client = MagicMock()
        mock_client.__aenter__.return_value.post = mock_post

        with patch(
            "server.llm_router.accounting.httpx.AsyncClient",
            return_value=mock_client,
        ):
            await self._call(cache_hit=True, cost_usd=0.0, prompt_tokens=50)

        _, kwargs = mock_post.call_args
        params = kwargs.get("params", {})
        query = params.get("query", "")
        # cached_tokens should equal prompt_tokens (50)
        assert ", 50, 0.0, 1," in query or "'50', 50, 50, 0.0, 1" in query

    @pytest.mark.asyncio
    async def test_logs_warning_on_http_error(self, caplog):
        """When ClickHouse returns a non-200 status, a warning is logged."""
        caplog.set_level(logging.WARNING)

        mock_post = AsyncMock()
        mock_post.return_value = MagicMock(status_code=500, text="Internal Error")

        mock_client = MagicMock()
        mock_client.__aenter__.return_value.post = mock_post

        with patch(
            "server.llm_router.accounting.httpx.AsyncClient",
            return_value=mock_client,
        ):
            await self._call()

        assert any("ClickHouse" in msg and "500" in msg for msg in caplog.messages)

    @pytest.mark.asyncio
    async def test_handles_clickhouse_unavailable(self, caplog):
        """record_call does not crash when ClickHouse is unreachable."""
        caplog.set_level(logging.WARNING)

        mock_post = AsyncMock()
        mock_post.side_effect = RuntimeError("Connection refused")

        mock_client = MagicMock()
        mock_client.__aenter__.return_value.post = mock_post

        with patch(
            "server.llm_router.accounting.httpx.AsyncClient",
            return_value=mock_client,
        ):
            # Must not raise
            await self._call()

        assert any("ClickHouse" in msg for msg in caplog.messages)


# ---------------------------------------------------------------------------
# record_metrics — in-memory counters
# ---------------------------------------------------------------------------


class TestRecordMetrics:
    """record_metrics() updates in-memory counters."""

    def test_cache_hit_increments_hit_counter(self):
        """A cache-hit call increments the hit counter."""
        record_metrics(
            provider="anthropic",
            model="claude-sonnet-5",
            purpose="chat",
            cache_hit=True,
            cost_usd=0.0,
        )
        assert metrics_module._cache_hits == 1
        assert metrics_module._cache_misses == 0

    def test_cache_miss_increments_miss_counter(self):
        """A cache-miss call increments the miss counter."""
        record_metrics(
            provider="deepseek",
            model="deepseek-chat",
            purpose="extract",
            cache_hit=False,
            cost_usd=0.001,
        )
        assert metrics_module._cache_misses == 1
        assert metrics_module._cache_hits == 0

    def test_multiple_calls_accumulate(self):
        """Multiple calls correctly accumulate both hit and miss counts."""
        record_metrics("anthropic", "sonnet", "chat", True, 0.0)
        record_metrics("deepseek", "chat", "extract", False, 0.001)
        record_metrics("anthropic", "haiku", "summarize", True, 0.0)

        assert metrics_module._cache_hits == 2
        assert metrics_module._cache_misses == 1

    def test_cost_accumulates_by_provider_model(self):
        """Costs are accumulated per (provider, model, purpose) tuple."""
        record_metrics("anthropic", "claude-sonnet-5", "chat", False, 0.003)
        record_metrics("anthropic", "claude-sonnet-5", "chat", False, 0.002)
        record_metrics("deepseek", "deepseek-chat", "extract", False, 0.001)

        assert metrics_module._cost_by_provider_model_purpose[
            ("anthropic", "claude-sonnet-5", "chat")
        ] == pytest.approx(0.005)
        assert metrics_module._cost_by_provider_model_purpose[
            ("deepseek", "deepseek-chat", "extract")
        ] == pytest.approx(0.001)

    def test_calls_by_purpose_tracks_count(self):
        """Each purpose's call count is tracked separately."""
        record_metrics("anthropic", "haiku", "summarize", False, 0.001)
        record_metrics("anthropic", "haiku", "summarize", False, 0.001)
        record_metrics("anthropic", "sonnet", "chat", False, 0.003)

        assert metrics_module._calls_by_purpose["summarize"] == 2
        assert metrics_module._calls_by_purpose["chat"] == 1


# ---------------------------------------------------------------------------
# get_metrics_text — Prometheus-format output
# ---------------------------------------------------------------------------


class TestGetMetricsText:
    """get_metrics_text() returns valid Prometheus-format text."""

    def test_empty_state_returns_zero_ratio(self):
        """With no calls, cache_hit_ratio is 0.0."""
        text = get_metrics_text()
        assert "aether_llm_cache_hit_ratio 0.0" in text

    def test_includes_cache_hit_ratio(self):
        """After some calls, the ratio is correctly reported."""
        record_metrics("anthropic", "haiku", "chat", True, 0.0)
        record_metrics("anthropic", "haiku", "chat", False, 0.001)
        record_metrics("anthropic", "haiku", "chat", False, 0.001)

        text = get_metrics_text()
        # 1 hit / 3 total = 0.333...
        assert "aether_llm_cache_hit_ratio 0.333" in text

    def test_includes_calls_total_by_purpose(self):
        """The calls_total metric is present with purpose labels."""
        record_metrics("deepseek", "chat", "extract", False, 0.001)
        record_metrics("deepseek", "chat", "extract", False, 0.001)
        record_metrics("anthropic", "sonnet", "chat", False, 0.003)

        text = get_metrics_text()
        assert 'aether_llm_calls_total{purpose="chat"} 1' in text
        assert 'aether_llm_calls_total{purpose="extract"} 2' in text

    def test_includes_cost_by_provider_model_purpose(self):
        """The cost metric is present with provider/model/purpose labels."""
        record_metrics("anthropic", "claude-sonnet-5", "chat", False, 0.003)
        record_metrics("deepseek", "deepseek-chat", "extract", False, 0.001)

        text = get_metrics_text()
        assert 'provider="anthropic"' in text
        assert 'model="claude-sonnet-5"' in text
        assert 'purpose="chat"' in text
        assert "0.003" in text

    def test_output_begins_with_help_and_type(self):
        """Prometheus format requires # HELP and # TYPE lines."""
        text = get_metrics_text()
        comment_lines = [ln for ln in text.split("\n") if ln.startswith("#")]
        assert any(
            "HELP" in ln and "aether_llm_cache_hit_ratio" in ln for ln in comment_lines
        )
        assert any(
            "TYPE" in ln and "aether_llm_cache_hit_ratio" in ln for ln in comment_lines
        )
        assert any(
            "HELP" in ln and "aether_llm_calls_total" in ln for ln in comment_lines
        )
        assert any(
            "TYPE" in ln and "aether_llm_calls_total" in ln for ln in comment_lines
        )
        assert any(
            "HELP" in ln and "aether_llm_cost_usd_total" in ln for ln in comment_lines
        )
        assert any(
            "TYPE" in ln and "aether_llm_cost_usd_total" in ln for ln in comment_lines
        )

    def test_multiple_values_have_newlines(self):
        """Each metric value is on its own line."""
        record_metrics("anthropic", "haiku", "summarize", False, 0.001)
        record_metrics("anthropic", "sonnet", "chat", False, 0.003)

        text = get_metrics_text()
        lines = text.strip().split("\n")
        # Should have # HELP, # TYPE, value lines plus blank lines
        value_lines = [ln for ln in lines if ln and not ln.startswith("#")]
        assert len(value_lines) >= 3  # ratio + 2 purpose lines + cost lines

    def test_is_plain_text(self):
        """Output is a plain string, not bytes or structured data."""
        text = get_metrics_text()
        assert isinstance(text, str)


# ---------------------------------------------------------------------------
# Integration: complete() returns trace_id
# ---------------------------------------------------------------------------


class TestCompleteTraceId:
    """complete() returns trace_id in the response."""

    @pytest.mark.asyncio
    @patch("server.llm_router.router.record_call", new_callable=AsyncMock)
    @patch("server.llm_router.router.litellm.acompletion", new_callable=AsyncMock)
    @patch("server.llm_router.router.get_api_keys")
    async def test_complete_returns_trace_id(
        self, mock_get_keys, mock_acompletion, mock_record_call
    ):
        """The response from complete() includes a non-empty trace_id string."""
        from server.llm_router.router import complete

        mock_get_keys.return_value = {"anthropic": "sk-test"}
        mock_acompletion.return_value = MagicMock(
            choices=[MagicMock(message=MagicMock(content="ok"))],
            usage=MagicMock(prompt_tokens=10, completion_tokens=20),
            _cost=0.001,
        )

        result = await complete(
            "chat",
            messages=[{"role": "user", "content": "hi"}],
            model_policy="default",
        )

        assert "trace_id" in result
        assert isinstance(result["trace_id"], str)
        assert len(result["trace_id"]) == 32  # uuid4 hex is 32 chars

    @pytest.mark.asyncio
    @patch("server.llm_router.router.record_call", new_callable=AsyncMock)
    @patch("server.llm_router.router.get_api_keys")
    async def test_complete_returns_trace_id_on_missing_key(
        self, mock_get_keys, mock_record_call
    ):
        """Even on missing key, trace_id is present."""
        from server.llm_router.router import complete

        mock_get_keys.return_value = {"anthropic": None}

        result = await complete(
            "chat",
            messages=[{"role": "user", "content": "hi"}],
            model_policy="default",
        )

        assert "trace_id" in result
        assert isinstance(result["trace_id"], str)
        assert len(result["trace_id"]) == 32

    @pytest.mark.asyncio
    @patch("server.llm_router.router.record_call", new_callable=AsyncMock)
    @patch("server.llm_router.router.litellm.acompletion", new_callable=AsyncMock)
    @patch("server.llm_router.router.get_api_keys")
    async def test_complete_returns_trace_id_on_litellm_error(
        self, mock_get_keys, mock_acompletion, mock_record_call
    ):
        """Even on LiteLLM error, trace_id is present."""
        from server.llm_router.router import complete

        mock_get_keys.return_value = {"openai": "sk-test"}
        mock_acompletion.side_effect = RuntimeError("API error")

        result = await complete(
            "chat",
            messages=[{"role": "user", "content": "hi"}],
            model_policy="default",
        )

        assert "trace_id" in result
        assert isinstance(result["trace_id"], str)
        assert len(result["trace_id"]) == 32
