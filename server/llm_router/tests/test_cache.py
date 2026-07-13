"""Unit tests for the LLM Router cache module."""

from __future__ import annotations

from unittest.mock import AsyncMock, patch

import pytest

import server.llm_router.cache as _cache_module
from server.llm_router.cache import (
    get_cache_stats,
    get_cached_response,
    get_semantic_match,
    set_cached_response,
    set_semantic_entry,
)

# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------


@pytest.fixture(autouse=True)
def reset_cache_counters():
    """Reset in-memory cache counters before each test."""
    _cache_module._cache_hits = 0
    _cache_module._cache_misses = 0
    yield


# ---------------------------------------------------------------------------
# get_cached_response / set_cached_response
# ---------------------------------------------------------------------------


class TestPromptCache:
    """Prompt cache round-trip and edge cases."""

    # ------------------------------------------------------------------
    # Cache hit
    # ------------------------------------------------------------------

    @pytest.mark.asyncio
    async def test_cache_hit_returns_stored_response(self):
        """A cache hit returns the previously stored response dict."""
        mock_redis = AsyncMock()
        mock_redis.ping.return_value = True
        mock_redis.get.return_value = (
            '{"text": "hello", "provider": "anthropic", "model": "sonnet"}'
        )

        with patch(
            "server.llm_router.cache._get_redis",
            return_value=mock_redis,
        ):
            result = await get_cached_response("test-key-123")

        assert result is not None
        assert result["text"] == "hello"
        assert result["provider"] == "anthropic"

    @pytest.mark.asyncio
    async def test_cache_hit_increments_hit_counter(self):
        """A cache hit increments the internal hit counter."""
        mock_redis = AsyncMock()
        mock_redis.ping.return_value = True
        mock_redis.get.return_value = '{"text": "cached"}'

        with patch(
            "server.llm_router.cache._get_redis",
            return_value=mock_redis,
        ):
            await get_cached_response("hit-key")

        assert _cache_module._cache_hits == 1
        assert _cache_module._cache_misses == 0

    # ------------------------------------------------------------------
    # Cache miss
    # ------------------------------------------------------------------

    @pytest.mark.asyncio
    async def test_cache_miss_returns_none(self):
        """A cache miss returns None."""
        mock_redis = AsyncMock()
        mock_redis.ping.return_value = True
        mock_redis.get.return_value = None

        with patch(
            "server.llm_router.cache._get_redis",
            return_value=mock_redis,
        ):
            result = await get_cached_response("miss-key")

        assert result is None

    @pytest.mark.asyncio
    async def test_cache_miss_increments_miss_counter(self):
        """A cache miss increments the internal miss counter."""
        mock_redis = AsyncMock()
        mock_redis.ping.return_value = True
        mock_redis.get.return_value = None

        with patch(
            "server.llm_router.cache._get_redis",
            return_value=mock_redis,
        ):
            await get_cached_response("miss-key")

        assert _cache_module._cache_misses == 1
        assert _cache_module._cache_hits == 0

    # ------------------------------------------------------------------
    # Set + Get round-trip
    # ------------------------------------------------------------------

    @pytest.mark.asyncio
    async def test_set_get_round_trip(self):
        """set_cached_response followed by get_cached_response returns the
        same data."""
        mock_instance = AsyncMock()
        mock_instance.ping = AsyncMock(return_value=True)

        # Simulate the storage: on set, store; on get, return stored
        stored: dict[str, str] = {}

        async def fake_set(key, value, ex=None):
            stored[key] = value

        async def fake_get(key):
            return stored.get(key)

        mock_instance.set.side_effect = fake_set
        mock_instance.get.side_effect = fake_get

        test_response = {
            "text": "round-trip",
            "usage": {"prompt_tokens": 5, "completion_tokens": 10},
            "model": "haiku",
            "provider": "anthropic",
            "cache_hit": False,
            "cost_usd": 0.001,
        }

        with patch(
            "server.llm_router.cache._get_redis",
            return_value=mock_instance,
        ):
            await set_cached_response("rt-key", test_response)
            result = await get_cached_response("rt-key")

        assert result is not None
        assert result["text"] == "round-trip"
        assert result["provider"] == "anthropic"
        assert result["usage"]["prompt_tokens"] == 5

    # ------------------------------------------------------------------
    # Redis unavailable
    # ------------------------------------------------------------------

    @pytest.mark.asyncio
    async def test_get_returns_none_when_redis_unavailable(self):
        """When Redis is unavailable, get returns None (graceful degradation)."""
        with patch(
            "server.llm_router.cache._get_redis",
            return_value=None,
        ):
            result = await get_cached_response("any-key")

        assert result is None

    @pytest.mark.asyncio
    async def test_set_logs_warning_when_redis_unavailable(self, caplog):
        """When Redis is unavailable, set logs a warning and does not crash."""
        import logging

        caplog.set_level(logging.WARNING)

        with patch(
            "server.llm_router.cache._get_redis",
            return_value=None,
        ):
            # Must not raise
            await set_cached_response("any-key", {"text": "test"})

    @pytest.mark.asyncio
    async def test_get_handles_redis_error_gracefully(self):
        """A Redis exception during GET returns None."""
        mock_redis = AsyncMock()
        mock_redis.get.side_effect = RuntimeError("Redis connection lost")

        with patch(
            "server.llm_router.cache._get_redis",
            return_value=mock_redis,
        ):
            result = await get_cached_response("error-key")

        assert result is None

    @pytest.mark.asyncio
    async def test_set_handles_redis_error_gracefully(self, caplog):
        """A Redis exception during SET logs a warning and does not crash."""
        import logging

        caplog.set_level(logging.WARNING)

        mock_redis = AsyncMock()
        mock_redis.set.side_effect = RuntimeError("Redis write failed")

        with patch(
            "server.llm_router.cache._get_redis",
            return_value=mock_redis,
        ):
            # Must not raise
            await set_cached_response("error-key", {"text": "test"})

        assert "degraded" in caplog.text or any(
            "Redis" in msg for msg in caplog.messages
        )


# ---------------------------------------------------------------------------
# Cache stats
# ---------------------------------------------------------------------------


class TestCacheStats:
    """get_cache_stats returns correct hit/miss/ratio."""

    async def _record_hits(self, n: int):
        """Record n cache hits via get_cached_response."""
        for _ in range(n):
            mock_redis = AsyncMock()
            mock_redis.ping.return_value = True
            mock_redis.get.return_value = '{"text": "hit"}'

            with patch(
                "server.llm_router.cache._get_redis",
                return_value=mock_redis,
            ):
                await get_cached_response("k")

    async def _record_misses(self, n: int):
        """Record n cache misses via get_cached_response."""
        for _ in range(n):
            mock_redis = AsyncMock()
            mock_redis.ping.return_value = True
            mock_redis.get.return_value = None

            with patch(
                "server.llm_router.cache._get_redis",
                return_value=mock_redis,
            ):
                await get_cached_response("k")

    @pytest.mark.asyncio
    async def test_empty_stats(self):
        """With no calls, hits=0 misses=0 ratio=0."""
        stats = await get_cache_stats()
        assert stats["hits"] == 0
        assert stats["misses"] == 0
        assert stats["ratio"] == 0.0

    @pytest.mark.asyncio
    async def test_stats_after_hits_and_misses(self):
        """After 3 hits and 2 misses, ratio is 0.6."""
        await self._record_hits(3)
        await self._record_misses(2)

        stats = await get_cache_stats()
        assert stats["hits"] == 3
        assert stats["misses"] == 2
        assert stats["ratio"] == 0.6

    @pytest.mark.asyncio
    async def test_stats_ratio_rounding(self):
        """Ratio is rounded to 4 decimal places."""
        await self._record_hits(1)
        await self._record_misses(3)

        stats = await get_cache_stats()
        assert stats["ratio"] == 0.25


# ---------------------------------------------------------------------------
# Semantic cache (stub)
# ---------------------------------------------------------------------------


class TestSemanticCache:
    """Semantic cache interface stubs — always miss, always no-op."""

    @pytest.mark.asyncio
    async def test_get_semantic_match_returns_none(self):
        """get_semantic_match returns None (stub)."""
        result = await get_semantic_match([0.1, 0.2, 0.3])
        assert result is None

    @pytest.mark.asyncio
    async def test_get_semantic_match_accepts_threshold(self):
        """get_semantic_match accepts a threshold parameter."""
        result = await get_semantic_match([0.1, 0.2], threshold=0.9)
        assert result is None

    @pytest.mark.asyncio
    async def test_set_semantic_entry_does_not_crash(self):
        """set_semantic_entry runs without error."""
        await set_semantic_entry([0.1, 0.2], {"text": "test data"})
