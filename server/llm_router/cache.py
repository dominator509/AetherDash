"""Redis-backed prompt cache and semantic cache for the LLM Router.

Prompt cache
    Full-response cache keyed by a SHA-256 hash of the prompt content.
    Used by ``router.complete()`` to short-circuit LiteLLM calls.

Semantic cache
    Interface landed now; full implementation deferred (Phase 3 leaning).
    The stub returns ``None`` (always miss) and logs a debug message.

Graceful degradation
    If Redis is unavailable all cache methods degrade to no-ops and log a
    warning.  The router continues to work without caching.
"""

from __future__ import annotations

import json
import logging
import os
from typing import Any

logger = logging.getLogger("aether.llm_router.cache")

# ---------------------------------------------------------------------------
# Redis client (lazy, best-effort)
# ---------------------------------------------------------------------------

_REDIS_URL: str = os.environ.get(
    "AETHER_REDIS__URL",
    "redis://localhost:6379/0",
)

_redis_available: bool | None = None
"""Cached availability flag.  ``None`` = not yet checked."""

_redis_instance: Any | None = None
"""Lazy Redis connection (redis.asyncio.Redis)."""


async def _get_redis() -> Any | None:
    """Return a Redis connection, or ``None`` if Redis is unavailable."""
    # pylint: disable=global-statement
    global _redis_available, _redis_instance  # noqa: PLW0603

    if _redis_available is False:
        return None

    if _redis_instance is not None:
        return _redis_instance

    try:
        import redis.asyncio as aioredis  # type: ignore[import-untyped]

        r = aioredis.from_url(
            _REDIS_URL, socket_connect_timeout=2, decode_responses=True
        )
        await r.ping()
        _redis_instance = r
        _redis_available = True
        logger.info("Connected to Redis at %s", _REDIS_URL)
        return r
    except Exception:
        logger.warning("Redis unavailable — cache degraded (no-op)", exc_info=True)
        _redis_available = False
        _redis_instance = None
        return None


async def _close_redis() -> None:
    """Close the Redis connection if open."""
    global _redis_available, _redis_instance  # noqa: PLW0603
    if _redis_instance is not None:
        try:
            await _redis_instance.aclose()
        except Exception:
            pass
    _redis_instance = None
    _redis_available = None


# ---------------------------------------------------------------------------
# Internal cache-stats counters (in-memory, reset on restart)
# ---------------------------------------------------------------------------

_cache_hits: int = 0
_cache_misses: int = 0


# ---------------------------------------------------------------------------
# Prompt cache
# ---------------------------------------------------------------------------

_PROMPT_CACHE_PREFIX: str = "aether_llm:prompt_cache:"
"""Redis key prefix for prompt-cache entries."""

_PROMPT_CACHE_TTL: int = 3600
"""Default TTL in seconds for prompt-cache entries (1 hour)."""


async def get_cached_response(cache_key: str) -> dict | None:
    """Check Redis for a cached response.

    Args:
        cache_key: The prompt's SHA-256 hex digest (from
            ``PromptAssembly.cache_key``).

    Returns:
        The cached response dict, or ``None`` on miss.
    """
    global _cache_hits, _cache_misses, _redis_available, _redis_instance  # noqa: PLW0603

    r = await _get_redis()
    if r is None:
        _cache_misses += 1
        return None

    try:
        raw = await r.get(f"{_PROMPT_CACHE_PREFIX}{cache_key}")
        if raw is not None:
            _cache_hits += 1
            logger.debug("Prompt cache HIT  key=%s", cache_key[:16])
            return json.loads(raw)
        _cache_misses += 1
        logger.debug("Prompt cache MISS key=%s", cache_key[:16])
        return None
    except Exception:
        logger.warning("Prompt cache GET failed (degraded)", exc_info=True)
        _redis_instance = None
        _redis_available = False
        _cache_misses += 1
        return None


async def set_cached_response(
    cache_key: str,
    response: dict,
    ttl: int = _PROMPT_CACHE_TTL,
) -> None:
    """Store a response in Redis with TTL.

    Args:
        cache_key: The prompt's SHA-256 hex digest.
        response: The response dict to cache.
        ttl: Time-to-live in seconds (default 3600).
    """
    r = await _get_redis()
    if r is None:
        return

    try:
        raw = json.dumps(response)
        await r.set(f"{_PROMPT_CACHE_PREFIX}{cache_key}", raw, ex=ttl)
        logger.debug("Prompt cache SET  key=%s ttl=%d", cache_key[:16], ttl)
    except Exception:
        logger.warning("Prompt cache SET failed (degraded)", exc_info=True)


async def get_cache_stats() -> dict[str, Any]:
    """Return cache hit/miss/ratio from in-memory counters.

    Returns:
        A dict with keys ``hits``, ``misses``, ``ratio``.
    """
    total = _cache_hits + _cache_misses
    ratio = _cache_hits / total if total > 0 else 0.0
    return {
        "hits": _cache_hits,
        "misses": _cache_misses,
        "ratio": round(ratio, 4),
    }


def reset_cache_stats() -> None:
    """Reset in-memory cache counters to zero (for testing)."""
    # pylint: disable=global-statement
    global _cache_hits, _cache_misses  # noqa: PLW0603
    _cache_hits = 0
    _cache_misses = 0


# ---------------------------------------------------------------------------
# Semantic cache (stub — interface only)
# ---------------------------------------------------------------------------

_SEMANTIC_CACHE_PREFIX: str = "aether_llm:semantic_cache:"
"""Redis key prefix for semantic-cache entries."""

_SEMANTIC_CACHE_TTL: int = 86400
"""Default TTL in seconds for semantic-cache entries (24 hours)."""


async def get_semantic_match(
    embedding: list[float],
    threshold: float = 0.95,
) -> dict | None:
    """Check Redis for a semantically similar cached response.

    .. note::
        This is an **interface stub**.  The full implementation (embedding
        storage, cosine-similarity search) is deferred to Phase 3.

    Args:
        embedding: The query embedding vector.
        threshold: Minimum cosine-similarity threshold (default 0.95).

    Returns:
        A cached response dict, or ``None`` (always ``None`` for now).
    """
    logger.debug(
        "Semantic cache stub called (threshold=%.2f) — always miss (Phase 3)",
        threshold,
    )
    return None


async def set_semantic_entry(
    embedding: list[float],
    response: dict,
    ttl: int = _SEMANTIC_CACHE_TTL,
) -> None:
    """Store a semantic cache entry.

    .. note::
        This is an **interface stub**.  The full implementation is deferred
        to Phase 3.

    Args:
        embedding: The response embedding vector.
        response: The response dict to cache.
        ttl: Time-to-live in seconds (default 86400).
    """
    logger.debug("Semantic cache stub called — no-op (Phase 3)")
