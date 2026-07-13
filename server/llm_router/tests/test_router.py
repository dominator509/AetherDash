"""Unit tests for the LLM Router."""

from __future__ import annotations

from unittest.mock import AsyncMock, patch

import pytest

from server.llm_router.config import lookup
from server.llm_router.router import _cache_key, complete, embed

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

_FAKE_RESPONSE = None  # placeholder; replaced by mock


def _make_mock_acompletion(**overrides: str | int | float):
    """Build a minimal mock object that quacks like a LiteLLM response."""
    from types import SimpleNamespace

    usage = SimpleNamespace(
        prompt_tokens=overrides.get("prompt_tokens", 10),
        completion_tokens=overrides.get("completion_tokens", 20),
    )
    choice = SimpleNamespace(
        message=SimpleNamespace(content=overrides.get("content", "mock response"))
    )

    response = SimpleNamespace(
        choices=[choice],
        usage=usage,
        _cost=overrides.get("cost", 0.0015),
    )
    return response


# ---------------------------------------------------------------------------
# Routing table tests
# ---------------------------------------------------------------------------


class TestLookup:
    """Verify that lookup() returns the correct (provider, model) pairs."""

    def test_summarize_default(self):
        assert lookup("summarize", "default") == (
            "anthropic",
            "claude-haiku-4-5-20251001",
        )

    def test_summarize_cheap(self):
        assert lookup("summarize", "cheap") == ("deepseek", "deepseek-chat")

    def test_summarize_local(self):
        assert lookup("summarize", "local") == ("ollama", "llama3.2:3b")

    def test_extract_default(self):
        assert lookup("extract", "default") == (
            "anthropic",
            "claude-haiku-4-5-20251001",
        )

    def test_extract_cheap(self):
        assert lookup("extract", "cheap") == ("deepseek", "deepseek-chat")

    def test_extract_local(self):
        assert lookup("extract", "local") == ("ollama", "llama3.2:3b")

    def test_embed_default(self):
        assert lookup("embed", "default") == (
            "openai",
            "text-embedding-3-small",
        )

    def test_embed_cheap(self):
        assert lookup("embed", "cheap") == ("ollama", "nomic-embed-text")

    def test_embed_local(self):
        assert lookup("embed", "local") == ("ollama", "nomic-embed-text")

    def test_chat_default(self):
        assert lookup("chat", "default") == ("anthropic", "claude-sonnet-5")

    def test_chat_cheap(self):
        assert lookup("chat", "cheap") == ("deepseek", "deepseek-chat")

    def test_chat_local(self):
        assert lookup("chat", "local") == ("ollama", "llama3.2:3b")

    def test_chat_xai(self):
        assert lookup("chat", "xai") == ("xai", "grok-4.5")

    def test_embed_rejects_xai_policy(self):
        with pytest.raises(ValueError, match="not supported for purpose"):
            lookup("embed", "xai")

    def test_unknown_purpose_raises(self):
        with pytest.raises(ValueError, match="Unknown purpose"):
            lookup("nonexistent", "default")

    def test_unknown_policy_raises(self):
        with pytest.raises(ValueError, match="Unknown model_policy"):
            lookup("chat", "nonexistent")


# ---------------------------------------------------------------------------
# complete() tests
# ---------------------------------------------------------------------------


class TestComplete:
    """Test the router's complete() function with mocked LiteLLM."""

    @pytest.mark.asyncio
    @pytest.mark.parametrize(
        ("purpose", "policy", "provider", "model"),
        [
            ("chat", "default", "anthropic", "claude-sonnet-5"),
            ("chat", "cheap", "deepseek", "deepseek-chat"),
            ("chat", "xai", "xai", "grok-4.5"),
            ("chat", "local", "ollama", "llama3.2:3b"),
        ],
    )
    @patch("server.llm_router.router.record_call", new_callable=AsyncMock)
    @patch("server.llm_router.router.get_cached_response", new_callable=AsyncMock)
    @patch("server.llm_router.router.litellm.acompletion", new_callable=AsyncMock)
    @patch("server.llm_router.router.get_api_keys")
    async def test_completion_provider_paths(
        self,
        mock_keys,
        mock_acompletion,
        mock_cache,
        _mock_record,
        purpose,
        policy,
        provider,
        model,
    ):
        mock_keys.return_value = {
            "anthropic": "test",
            "deepseek": "test",
            "xai": "test",
            "openai": "test",
        }
        mock_cache.return_value = None
        mock_acompletion.return_value = _make_mock_acompletion()
        result = await complete(
            purpose, [{"role": "user", "content": "stub request"}], policy
        )
        assert result["provider"] == provider
        mock_acompletion.assert_awaited_once_with(
            model=f"{provider}/{model}",
            messages=[{"role": "user", "content": "stub request"}],
        )

    @pytest.mark.asyncio
    @patch("server.llm_router.router.record_call", new_callable=AsyncMock)
    @patch("server.llm_router.router.litellm.acompletion", new_callable=AsyncMock)
    @patch("server.llm_router.router.get_api_keys")
    async def test_calls_litellm_with_correct_provider_model(
        self, mock_get_keys, mock_acompletion, mock_record_call
    ):
        """Verify that complete() sends the correct provider/model to LiteLLM."""
        mock_get_keys.return_value = {"anthropic": "sk-test"}
        fake_resp = _make_mock_acompletion(content="Hello from Claude")
        mock_acompletion.return_value = fake_resp

        result = await complete(
            "chat",
            messages=[{"role": "user", "content": "hi"}],
            model_policy="default",
        )

        mock_acompletion.assert_awaited_once_with(
            model="anthropic/claude-sonnet-5",
            messages=[{"role": "user", "content": "hi"}],
        )
        assert result["text"] == "Hello from Claude"
        assert result["model"] == "claude-sonnet-5"
        assert result["provider"] == "anthropic"
        assert isinstance(result["trace_id"], str) and len(result["trace_id"]) > 0

    @pytest.mark.asyncio
    @patch("server.llm_router.router.record_call", new_callable=AsyncMock)
    @patch("server.llm_router.router.litellm.acompletion", new_callable=AsyncMock)
    @patch("server.llm_router.router.get_api_keys")
    async def test_returns_expected_response_shape(
        self, mock_get_keys, mock_acompletion, mock_record_call
    ):
        """Verify the response dict has the expected keys and types."""
        mock_get_keys.return_value = {"deepseek": "sk-test"}
        fake_resp = _make_mock_acompletion(
            content="extracted data",
            prompt_tokens=15,
            completion_tokens=25,
            cost=0.002,
        )
        mock_acompletion.return_value = fake_resp

        result = await complete(
            "extract",
            messages=[{"role": "user", "content": "parse this"}],
            model_policy="cheap",
        )

        assert isinstance(result, dict)
        assert isinstance(result["text"], str)
        assert isinstance(result["usage"], dict)
        assert "prompt_tokens" in result["usage"]
        assert "completion_tokens" in result["usage"]
        assert isinstance(result["model"], str)
        assert isinstance(result["provider"], str)
        assert isinstance(result["cache_hit"], bool)
        assert isinstance(result["cost_usd"], float)
        assert isinstance(result["trace_id"], str) and len(result["trace_id"]) > 0
        assert result["usage"]["prompt_tokens"] == 15
        assert result["usage"]["completion_tokens"] == 25
        assert result["cost_usd"] == 0.002

    @pytest.mark.asyncio
    @patch("server.llm_router.router.record_call", new_callable=AsyncMock)
    @patch("server.llm_router.router.litellm.acompletion", new_callable=AsyncMock)
    @patch("server.llm_router.router.get_api_keys")
    async def test_handles_litellm_error_gracefully(
        self, mock_get_keys, mock_acompletion, mock_record_call
    ):
        """When LiteLLM raises, complete() returns an error dict, doesn't crash."""
        mock_get_keys.return_value = {"openai": "sk-test"}
        mock_acompletion.side_effect = RuntimeError("API timeout")

        result = await complete(
            "embed",
            messages=[{"role": "user", "content": "embed this"}],
        )

        assert result["error"] == "provider_call_failed"
        assert result["text"] == "LLM provider call failed"
        assert result["usage"]["prompt_tokens"] == 0
        assert result["usage"]["completion_tokens"] == 0
        assert isinstance(result["trace_id"], str) and len(result["trace_id"]) > 0

    @pytest.mark.asyncio
    @patch("server.llm_router.router.get_cached_response", new_callable=AsyncMock)
    @patch("server.llm_router.router.get_api_keys")
    async def test_missing_api_key_returns_clear_message(
        self,
        mock_get_keys,
        mock_get_cached,
    ):
        """When a provider's API key is missing, return a clear error message."""
        mock_get_keys.return_value = {"anthropic": None}
        mock_get_cached.return_value = None

        result = await complete(
            "chat",
            messages=[{"role": "user", "content": "hi"}],
            model_policy="default",
        )

        assert "ANTHROPIC_API_KEY" in result["text"]
        assert result["usage"]["prompt_tokens"] == 0
        assert result["usage"]["completion_tokens"] == 0
        assert isinstance(result["trace_id"], str) and len(result["trace_id"]) > 0

    @pytest.mark.asyncio
    async def test_unknown_purpose_raises_value_error(self):
        """An unknown purpose should propagate as ValueError."""
        with pytest.raises(ValueError, match="Unknown purpose"):
            await complete(
                "invalid-purpose",
                messages=[{"role": "user", "content": "test"}],
            )

    def test_cache_key_includes_roles_purpose_and_policy(self):
        assert _cache_key(
            "chat", "default", [{"role": "user", "content": "ab"}]
        ) != _cache_key("chat", "default", [{"role": "assistant", "content": "ab"}])
        assert _cache_key(
            "chat", "default", [{"role": "user", "content": "ab"}]
        ) != _cache_key("extract", "default", [{"role": "user", "content": "ab"}])
        assert _cache_key(
            "chat",
            "default",
            [{"role": "user", "content": "a"}, {"role": "user", "content": "b"}],
        ) != _cache_key("chat", "default", [{"role": "user", "content": "ab"}])

    @pytest.mark.asyncio
    @patch("server.llm_router.router.record_metrics")
    @patch("server.llm_router.router.record_call", new_callable=AsyncMock)
    @patch("server.llm_router.router.get_cached_response", new_callable=AsyncMock)
    async def test_cache_hit_is_accounted(
        self, mock_cache, mock_record_call, mock_record_metrics
    ):
        mock_cache.return_value = {
            "text": "cached",
            "usage": {"prompt_tokens": 9, "completion_tokens": 4},
            "provider": "anthropic",
            "model": "haiku",
            "cache_hit": False,
            "cost_usd": 0.1,
        }
        result = await complete("chat", [{"role": "user", "content": "cached request"}])
        assert result["cache_hit"] is True
        assert result["cost_usd"] == 0.0
        mock_record_call.assert_awaited_once()
        assert mock_record_call.await_args.kwargs["cache_hit"] is True
        mock_record_metrics.assert_called_once()


class TestEmbed:
    @pytest.mark.asyncio
    @patch("server.llm_router.router.record_metrics")
    @patch("server.llm_router.router.record_call", new_callable=AsyncMock)
    @patch("server.llm_router.router.litellm.aembedding", new_callable=AsyncMock)
    @patch("server.llm_router.router.get_api_keys")
    async def test_uses_embedding_api(
        self, mock_keys, mock_embedding, mock_record_call, _mock_metrics
    ):
        from types import SimpleNamespace

        mock_keys.return_value = {"openai": "test-key"}
        mock_embedding.return_value = SimpleNamespace(
            data=[{"embedding": [0.1, 0.2, 0.3]}],
            usage=SimpleNamespace(prompt_tokens=3),
            _cost=0.0001,
        )
        result = await embed("hello")
        assert result["embedding"] == [0.1, 0.2, 0.3]
        mock_embedding.assert_awaited_once_with(
            model="openai/text-embedding-3-small", input=["hello"]
        )
        mock_record_call.assert_awaited_once()
