"""ClickHouse accounting for LLM Router calls.

Writes an ``llm_calls`` row per call via the ClickHouse HTTP interface.
Best-effort: failures are logged at WARNING level and never propagated.
"""

from __future__ import annotations

import logging
import os
from datetime import UTC, datetime

import httpx

logger = logging.getLogger("aether.llm_router.accounting")

CLICKHOUSE_URL = os.environ.get("AETHER_CLICKHOUSE__URL", "http://localhost:8123")
CLICKHOUSE_DB = os.environ.get("AETHER_CLICKHOUSE__DATABASE", "aether")
CLICKHOUSE_USER = os.environ.get("AETHER_CLICKHOUSE__USER", "aether")
CLICKHOUSE_PASSWORD = os.environ.get("AETHER_CLICKHOUSE__PASSWORD", "aether")

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _escape_sql(val: str) -> str:
    """Escape a single-quoted string for ClickHouse SQL."""
    return val.replace("'", "''")


# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------


async def record_call(
    purpose: str,
    provider: str,
    model: str,
    prompt_tokens: int,
    completion_tokens: int,
    cache_hit: bool,
    cost_usd: float,
    latency_ms: float,
    trace_id: str | None = None,
) -> None:
    """Write an ``llm_calls`` row to ClickHouse.

    This is **best-effort, fire-and-forget**.  If ClickHouse is unavailable the
    call is still considered successful — the error is logged and execution
    continues.  Cost is 0-or-residual (cache hits carry zero cost).

    Args:
        purpose: One of ``"summarize"``, ``"extract"``, ``"embed"``, ``"chat"``.
        provider: Provider name (e.g. ``"anthropic"``).
        model: Model name (e.g. ``"claude-sonnet-5"``).
        prompt_tokens: Number of prompt tokens.
        completion_tokens: Number of completion tokens.
        cache_hit: Whether the response was served from cache.
        cost_usd: Dollar cost of the call (0.0 for cache hits).
        latency_ms: Wall-clock latency in milliseconds.
        trace_id: Optional trace identifier (uuid4 hex).
    """
    # The authoritative ClickHouse schema uses DateTime (second precision),
    # not DateTime64; fractional seconds make INSERT parsing fail.
    ts = datetime.now(UTC).strftime("%Y-%m-%d %H:%M:%S")

    cached_tokens = prompt_tokens if cache_hit else 0

    safe_purpose = _escape_sql(purpose)
    safe_provider = _escape_sql(provider)
    safe_model = _escape_sql(model)
    safe_trace_id = _escape_sql(trace_id or "")

    cache_hit_int = 1 if cache_hit else 0

    query = (
        f"INSERT INTO {CLICKHOUSE_DB}.llm_calls "
        "(ts, provider, model, purpose, prompt_tokens, completion_tokens, "
        "cached_tokens, cost_usd, cache_hit, latency_ms, trace_id) "
        "VALUES"
    )

    values = (
        f"('{ts}', '{safe_provider}', '{safe_model}', '{safe_purpose}', "
        f"{prompt_tokens}, {completion_tokens}, {cached_tokens}, "
        f"{cost_usd}, {cache_hit_int}, {max(0, round(latency_ms))}, "
        f"'{safe_trace_id}')"
    )

    try:
        async with httpx.AsyncClient() as client:
            response = await client.post(
                CLICKHOUSE_URL,
                params={"query": f"{query} {values}"},
                auth=(CLICKHOUSE_USER, CLICKHOUSE_PASSWORD),
                timeout=5.0,
            )
        if response.status_code != 200:
            logger.warning(
                "ClickHouse llm_calls INSERT returned status %d: %.200s",
                response.status_code,
                response.text,
            )
    except Exception:
        logger.warning(
            "ClickHouse llm_calls INSERT failed (best-effort, continuing):",
            exc_info=True,
        )
