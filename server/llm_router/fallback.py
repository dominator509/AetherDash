"""Fallback chain for the LLM Router.

When a provider is unavailable or returns a transient error the router
tries the next provider in the fallback chain rather than failing immediately.

Error classification
    *Timeout / rate-limit / 5xx* — transient, fallback to next in chain.
    *Auth / invalid API key* — fatal config error, do NOT fallback.
    *Context length exceeded* — input too large, do NOT fallback.
    *All providers exhausted* — return error dict.

Fallback chain
    default -> cheap -> local
    cheap   -> local
    local   -> error
"""

from __future__ import annotations

import logging
from typing import Any
from uuid import uuid4

from server.llm_router.router import complete

logger = logging.getLogger("aether.llm_router.fallback")

# ---------------------------------------------------------------------------
# Fallback chain
# ---------------------------------------------------------------------------

FALLBACK_CHAIN: dict[str, list[str]] = {
    "default": ["cheap", "local"],
    "cheap": ["local"],
    "local": [],
    "xai": ["cheap", "local"],
}
"""Map each model_policy to its ordered fallback list.

When the primary model_policy fails with a transient error, the router
tries each fallback in order.  An empty list means exhaustion.
"""

# ---------------------------------------------------------------------------
# Error classification
# ---------------------------------------------------------------------------

# Substrings that indicate a transient error — fallback is safe
_TRANSIENT_ERRORS: tuple[str, ...] = (
    "timeout",
    "timed out",
    "rate limit",
    "rate_limit",
    "429",
    "503",
    "502",
    "504",
    "service unavailable",
    "too many requests",
    "connection reset",
    "connection refused",
    "upstream",
)

# Substrings that indicate auth / config errors — do NOT fallback
_FATAL_ERRORS: tuple[str, ...] = (
    "401",
    "403",
    "unauthorized",
    "auth",
    "api key",
    "invalid key",
    "invalid authentication",
    "authentication failed",
    "permission denied",
    "not authorized",
    "forbidden",
)

# Substrings that indicate context-length issues — do NOT fallback
_CONTEXT_ERRORS: tuple[str, ...] = (
    "context length",
    "context_length",
    "too many tokens",
    "max tokens",
    "maximum context",
    "token limit",
)


def _classify_error(error_message: str) -> str:
    """Classify an error as ``"transient"``, ``"fatal"``, or
    ``"context"``.

    Args:
        error_message: The error text to classify.

    Returns:
        One of ``"transient"``, ``"fatal"``, ``"context"``.
    """
    lower = error_message.lower()

    if any(pat in lower for pat in _FATAL_ERRORS):
        return "fatal"

    if any(pat in lower for pat in _CONTEXT_ERRORS):
        return "context"

    if any(pat in lower for pat in _TRANSIENT_ERRORS):
        return "transient"

    # Default to transient for classified API errors (LiteLLM wraps
    # provider errors with descriptive text).  Unrecognised patterns
    # that look like API errors are still safer to treat as transient.
    return "transient"


# ---------------------------------------------------------------------------
# Fallback metrics counter
# ---------------------------------------------------------------------------

_fallback_total: int = 0
"""Total number of fallback events across all chains."""

_fallback_events: list[dict[str, str]] = []
"""History of fallback events for Prometheus-format metrics."""


def _record_fallback(from_provider: str, to_provider: str) -> None:
    """Record a fallback event in the in-memory counter.

    Intended for Prometheus: ``aether_llm_fallback_total{from,to}``.
    """
    global _fallback_total  # noqa: PLW0603
    _fallback_total += 1
    _fallback_events.append(
        {
            "from": from_provider,
            "to": to_provider,
        }
    )
    logger.info(
        "Fallback from provider=%s -> provider=%s (total=%d)",
        from_provider,
        to_provider,
        _fallback_total,
    )


def get_fallback_metrics_text() -> str:
    """Return Prometheus-format text for fallback metrics.

    Exported series
    ----------------
    * ``aether_llm_fallback_total{from_provider,to_provider}`` — counter
    """
    lines: list[str] = [
        "# HELP aether_llm_fallback_total Total LLM fallback events by"
        " source and destination provider",
        "# TYPE aether_llm_fallback_total counter",
    ]

    # Aggregate by (from, to)
    agg: dict[tuple[str, str], int] = {}
    for ev in _fallback_events:
        key = (ev["from"], ev["to"])
        agg[key] = agg.get(key, 0) + 1

    for (from_prov, to_prov), count in sorted(agg.items()):
        lines.append(
            f"aether_llm_fallback_total{{"
            f'from_provider="{from_prov}",to_provider="{to_prov}"'
            f"}} {count}"
        )

    lines.append("")
    return "\n".join(lines)


def reset_fallback_counters() -> None:
    """Reset in-memory fallback counters to zero (for testing)."""
    # pylint: disable=global-statement
    global _fallback_total  # noqa: PLW0603
    _fallback_total = 0
    _fallback_events.clear()


# ---------------------------------------------------------------------------
# Fallback-aware completion
# ---------------------------------------------------------------------------

_RESERVED_FALLBACK_KEYS: set[str] = {
    "text",
    "usage",
    "model",
    "provider",
    "cache_hit",
    "cost_usd",
    "trace_id",
}


def _lookup_provider(
    purpose: str,
    model_policy: str,
) -> str:
    """Return the provider name for a given purpose + model_policy.

    Uses the routing table from ``config.py``.
    """
    from server.llm_router.config import lookup  # noqa: PLC0415

    provider, _model = lookup(purpose, model_policy)
    return provider


async def complete_with_fallback(
    purpose: str,
    messages: list[dict[str, str]],
    model_policy: str = "default",
) -> dict[str, Any]:
    """Call ``complete()`` and fall back through the chain on transient errors.

    Args:
        purpose: One of ``"summarize"``, ``"extract"``, ``"embed"``, ``"chat"``.
        messages: OpenAI-style message list.
        model_policy: One of ``"default"``, ``"cheap"``, ``"local"``.

    Returns:
        A response dict (same shape as ``complete()``), or an error dict
        with ``{"error": "all providers exhausted"}`` if every provider in
        the chain failed.

    Raises:
        ValueError: If the purpose or model_policy is unknown.
    """
    chain = FALLBACK_CHAIN.get(model_policy, [])
    policies_to_try = [model_policy] + chain
    trace_id = uuid4().hex

    last_error_text: str = ""
    last_provider: str | None = None

    for idx, policy in enumerate(policies_to_try):
        try:
            result = await complete(
                purpose=purpose,
                messages=messages,
                model_policy=policy,
            )
        except ValueError:
            raise

        # Check for "LiteLLM call failed" in result text — the router
        # catches exceptions and returns them as error dicts.
        error_text = result.get("text", "")

        if result.get("error") or error_text.startswith("LiteLLM call failed"):
            classification = (
                "fatal"
                if result.get("error") == "provider_not_configured"
                else _classify_error(result.get("error_class", error_text))
            )

            if classification in ("fatal", "context"):
                logger.warning(
                    "Fallback chain halted at %s error=%s purpose=%s policy=%s",
                    classification,
                    error_text[:120],
                    purpose,
                    policy,
                )
                result["error"] = f"Non-retryable error: {error_text}"
                result["trace_id"] = trace_id
                return result

            # Transient — record the fallback and continue
            current_provider = result.get("provider", "unknown")
            if last_provider and current_provider != last_provider:
                _record_fallback(last_provider, current_provider)

            last_error_text = error_text
            last_provider = current_provider
            continue

        # Successful response
        current_provider = result.get("provider", "unknown")
        if last_provider and current_provider != last_provider:
            _record_fallback(last_provider, current_provider)

        result["trace_id"] = trace_id
        result["fallback_chain"] = policies_to_try[: idx + 1]
        return result

    # All providers exhausted
    logger.error(
        "All providers exhausted purpose=%s chain=%s last_error=%.200s",
        purpose,
        policies_to_try,
        last_error_text,
    )

    from server.llm_router.router import _DEFAULT_RESPONSE  # noqa: PLC0415

    result = dict(_DEFAULT_RESPONSE)
    result["text"] = (
        f"All providers exhausted: {last_error_text}"
        if last_error_text
        else "All providers exhausted"
    )
    result["error"] = "all providers exhausted"
    result["trace_id"] = trace_id
    result["fallback_chain"] = policies_to_try
    return result
