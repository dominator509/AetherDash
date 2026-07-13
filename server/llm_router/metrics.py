"""Prometheus-format metrics for the LLM Router.

In-memory counters updated per call.  Exported as ``text/plain`` for the
``/metrics`` endpoint.

Exported series
---------------
* ``aether_llm_cache_hit_ratio`` — gauge
* ``aether_llm_calls_total{purpose}`` — counter
* ``aether_llm_cost_usd_total{provider,model,purpose}`` — counter
"""

from __future__ import annotations

from collections import defaultdict

_cache_hits: int = 0
_cache_misses: int = 0
_cost_by_provider_model_purpose: dict[tuple[str, str, str], float] = defaultdict(float)
_calls_by_purpose: dict[str, int] = defaultdict(int)


def record_metrics(
    provider: str,
    model: str,
    purpose: str,
    cache_hit: bool,
    cost_usd: float,
) -> None:
    """Update in-memory metrics counters.

    Args:
        provider: Provider name (e.g. ``"anthropic"``).
        model: Model name (e.g. ``"claude-sonnet-5"``).
        purpose: Call purpose (``"summarize"``, ``"extract"``, etc.).
        cache_hit: Whether the response was served from cache.
        cost_usd: Dollar cost of the call.
    """
    # pylint: disable=global-statement
    global _cache_hits, _cache_misses  # noqa: PLW0603

    if cache_hit:
        _cache_hits += 1
    else:
        _cache_misses += 1

    _cost_by_provider_model_purpose[(provider, model, purpose)] += cost_usd
    _calls_by_purpose[purpose] += 1


def get_metrics_text() -> str:
    """Return Prometheus-format text for the ``/metrics`` endpoint."""
    total = _cache_hits + _cache_misses
    ratio = _cache_hits / total if total > 0 else 0.0

    lines: list[str] = [
        "# HELP aether_llm_cache_hit_ratio Cache hit ratio for LLM calls",
        "# TYPE aether_llm_cache_hit_ratio gauge",
        f"aether_llm_cache_hit_ratio {ratio}",
        "",
        "# HELP aether_llm_calls_total Total number of LLM calls by purpose",
        "# TYPE aether_llm_calls_total counter",
    ]

    for purpose, count in sorted(_calls_by_purpose.items()):
        lines.append(f'aether_llm_calls_total{{purpose="{purpose}"}} {count}')

    lines.append("")
    lines.append(
        "# HELP aether_llm_cost_usd_total Total cost of LLM calls by"
        " provider, model, and purpose"
    )
    lines.append("# TYPE aether_llm_cost_usd_total counter")

    for (provider, model, purpose), cost in sorted(
        _cost_by_provider_model_purpose.items()
    ):
        lines.append(
            f"aether_llm_cost_usd_total{{"
            f'provider="{provider}",model="{model}",purpose="{purpose}"'
            f"}} {cost}"
        )

    lines.append("")
    return "\n".join(lines)
