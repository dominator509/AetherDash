"""LLM Router core — LiteLLM completion dispatch."""

from __future__ import annotations

import hashlib
import json
import logging
import time
from typing import Any
from uuid import uuid4

import litellm

from server.llm_router.accounting import record_call
from server.llm_router.cache import get_cached_response, set_cached_response
from server.llm_router.config import (
    get_api_keys,
    get_ollama_base,
    lookup,
)
from server.llm_router.metrics import record_metrics

logger = logging.getLogger("aether.llm_router")

# ---------------------------------------------------------------------------
# LiteLLM global configuration
# ---------------------------------------------------------------------------

_api_keys = get_api_keys()
if _api_keys["anthropic"]:
    litellm.anthropic_key = _api_keys["anthropic"]
if _api_keys["deepseek"]:
    litellm.deepseek_key = _api_keys["deepseek"]
if _api_keys["openai"]:
    litellm.openai_key = _api_keys["openai"]
if _api_keys["xai"]:
    litellm.xai_key = _api_keys["xai"]

litellm.ollama_base_url = get_ollama_base()

# ---------------------------------------------------------------------------
# Response shape
# ---------------------------------------------------------------------------

_DEFAULT_RESPONSE: dict[str, Any] = {
    "text": "",
    "usage": {"prompt_tokens": 0, "completion_tokens": 0},
    "model": "",
    "provider": "",
    "cache_hit": False,
    "cost_usd": 0.0,
    "trace_id": "",
}


def _build_response(
    model: str,
    provider: str,
    response: Any,
    trace_id: str,
) -> dict[str, Any]:
    """Extract a standardised response dict from a LiteLLM response."""
    usage = getattr(response, "usage", None)

    message = response.choices[0].message
    raw_tool_calls = getattr(message, "tool_calls", None) or []
    tool_calls = [
        call.model_dump() if hasattr(call, "model_dump") else dict(call)
        for call in raw_tool_calls
    ]
    return {
        "text": message.content or "",
        "tool_calls": tool_calls,
        "usage": {
            "prompt_tokens": getattr(usage, "prompt_tokens", 0) or 0,
            "completion_tokens": getattr(usage, "completion_tokens", 0) or 0,
        },
        "model": model,
        "provider": provider,
        "cache_hit": False,
        "cost_usd": float(getattr(response, "_cost", 0) or 0),
        "trace_id": trace_id,
    }


def _missing_key_error(provider: str) -> dict[str, Any]:
    """Return a standardised error dict for missing API keys."""
    env_var_map = {
        "anthropic": "AETHER_LLM__ANTHROPIC_API_KEY",
        "deepseek": "AETHER_LLM__DEEPSEEK_API_KEY",
        "openai": "AETHER_LLM__OPENAI_API_KEY",
        "xai": "AETHER_LLM__XAI_API_KEY",
        "ollama": None,  # no API key needed for local models
    }
    key_var = env_var_map.get(provider)
    if key_var:
        msg = (
            f"Provider {provider!r} requires {key_var} environment variable, "
            f"which is not set. Set {key_var}=<your-api-key> and try again."
        )
    else:
        msg = (
            f"Provider {provider!r} does not require an API key. "
            f"Ensure the service is running at {get_ollama_base()}."
        )

    result = dict(_DEFAULT_RESPONSE)
    result["text"] = msg
    result["model"] = provider
    result["provider"] = provider
    result["error"] = "provider_not_configured"
    return result


def _cache_key(purpose: str, model_policy: str, messages: list[dict[str, str]]) -> str:
    """Hash the complete routing identity without ambiguous concatenation."""
    payload = {
        "purpose": purpose,
        "model_policy": model_policy,
        "messages": messages,
    }
    canonical = json.dumps(payload, sort_keys=True, separators=(",", ":"))
    return hashlib.sha256(canonical.encode("utf-8")).hexdigest()


async def complete(
    purpose: str,
    messages: list[dict[str, str]],
    model_policy: str = "default",
) -> dict[str, Any]:
    """Route a completion request through LiteLLM.

    Args:
        purpose: One of "summarize", "extract", "embed", "chat".
        messages: OpenAI-style message list, e.g.
            ``[{"role": "user", "content": "..."}]``.
        model_policy: One of "default", "cheap", "local".

    Returns:
        A dict with keys ``text``, ``usage``, ``model``, ``provider``,
        ``cache_hit``, ``cost_usd``.
    """
    provider, model = lookup(purpose, model_policy)
    trace_id = uuid4().hex

    # --- Build cache key (SHA-256 of message content) -----------------------
    cache_key = _cache_key(purpose, model_policy, messages)

    # --- Check prompt cache first -------------------------------------------
    cached = await get_cached_response(cache_key)
    if cached is not None:
        logger.info(
            "Cache HIT  purpose=%s provider=%s model=%s",
            purpose,
            cached.get("provider", provider),
            cached.get("model", model),
        )
        cached["cache_hit"] = True
        cached["cost_usd"] = 0.0
        cached["trace_id"] = trace_id
        usage = cached.get("usage", {})
        await record_call(
            purpose=purpose,
            provider=cached.get("provider", provider),
            model=cached.get("model", model),
            prompt_tokens=int(usage.get("prompt_tokens", 0)),
            completion_tokens=0,
            cache_hit=True,
            cost_usd=0.0,
            latency_ms=0.0,
            trace_id=trace_id,
        )
        record_metrics(
            provider=cached.get("provider", provider),
            model=cached.get("model", model),
            purpose=purpose,
            cache_hit=True,
            cost_usd=0.0,
        )
        return cached

    # --- Check API key availability ----------------------------------------
    api_keys = get_api_keys()
    if provider in api_keys and api_keys[provider] is None:
        logger.warning(
            "Missing API key for provider=%s purpose=%s policy=%s",
            provider,
            purpose,
            model_policy,
        )
        result = _missing_key_error(provider)
        result["trace_id"] = trace_id
        await record_call(
            purpose=purpose,
            provider=provider,
            model=model,
            prompt_tokens=0,
            completion_tokens=0,
            cache_hit=False,
            cost_usd=0.0,
            latency_ms=0.0,
            trace_id=trace_id,
        )
        record_metrics(provider, model, purpose, False, 0.0)
        return result

    # --- Call LiteLLM ------------------------------------------------------
    litellm_model = f"{provider}/{model}" if provider != "local" else model

    start = time.monotonic()
    try:
        response = await litellm.acompletion(
            model=litellm_model,
            messages=messages,
        )
    except Exception as exc:
        latency_ms = (time.monotonic() - start) * 1000
        logger.error(
            "LiteLLM call failed provider=%s model=%s purpose=%s error=%s",
            provider,
            model,
            purpose,
            type(exc).__name__,
        )
        result = dict(_DEFAULT_RESPONSE)
        result["text"] = "LLM provider call failed"
        result["error"] = "provider_call_failed"
        result["error_class"] = type(exc).__name__
        result["model"] = model
        result["provider"] = provider
        result["trace_id"] = trace_id

        # Record the failed call (best-effort, zero cost)
        await record_call(
            purpose=purpose,
            provider=provider,
            model=model,
            prompt_tokens=0,
            completion_tokens=0,
            cache_hit=False,
            cost_usd=0.0,
            latency_ms=latency_ms,
            trace_id=trace_id,
        )
        record_metrics(
            provider=provider,
            model=model,
            purpose=purpose,
            cache_hit=False,
            cost_usd=0.0,
        )
        return result

    latency_ms = (time.monotonic() - start) * 1000

    # --- Build response ----------------------------------------------------
    result = _build_response(model, provider, response, trace_id)

    # --- Store in prompt cache (best-effort) --------------------------------
    await set_cached_response(cache_key, result)

    logger.info(
        "complete purpose=%s model=%s provider=%s "
        "prompt_tokens=%d completion_tokens=%d cost_usd=%.6f",
        purpose,
        model,
        provider,
        result["usage"]["prompt_tokens"],
        result["usage"]["completion_tokens"],
        result["cost_usd"],
    )

    # --- Accounting + Metrics (best-effort, never crashes) -----------------
    await record_call(
        purpose=purpose,
        provider=provider,
        model=model,
        prompt_tokens=result["usage"]["prompt_tokens"],
        completion_tokens=result["usage"]["completion_tokens"],
        cache_hit=result["cache_hit"],
        cost_usd=result["cost_usd"],
        latency_ms=latency_ms,
        trace_id=trace_id,
    )
    record_metrics(
        provider=provider,
        model=model,
        purpose=purpose,
        cache_hit=result["cache_hit"],
        cost_usd=result["cost_usd"],
    )

    return result


async def embed(text: str, model_policy: str = "default") -> dict[str, Any]:
    """Route an embedding request through LiteLLM's embedding API."""
    provider, model = lookup("embed", model_policy)
    trace_id = uuid4().hex
    api_keys = get_api_keys()
    if provider in api_keys and api_keys[provider] is None:
        result = _missing_key_error(provider)
        result.update({"model": model, "trace_id": trace_id, "embedding": []})
        await record_call("embed", provider, model, 0, 0, False, 0.0, 0.0, trace_id)
        record_metrics(provider, model, "embed", False, 0.0)
        return result

    litellm_model = f"{provider}/{model}"
    start = time.monotonic()
    try:
        response = await litellm.aembedding(model=litellm_model, input=[text])
        vector = list(response.data[0]["embedding"])
        usage = getattr(response, "usage", None)
        prompt_tokens = int(getattr(usage, "prompt_tokens", 0) or 0)
        cost = float(getattr(response, "_cost", 0) or 0)
        result = {
            "embedding": vector,
            "model": model,
            "provider": provider,
            "usage": {"prompt_tokens": prompt_tokens, "completion_tokens": 0},
            "cache_hit": False,
            "cost_usd": cost,
            "trace_id": trace_id,
        }
    except Exception as exc:
        result = {
            "embedding": [],
            "model": model,
            "provider": provider,
            "usage": {"prompt_tokens": 0, "completion_tokens": 0},
            "cache_hit": False,
            "cost_usd": 0.0,
            "trace_id": trace_id,
            "error": "provider_call_failed",
            "error_class": type(exc).__name__,
        }
    latency_ms = (time.monotonic() - start) * 1000
    usage = result["usage"]
    await record_call(
        "embed",
        provider,
        model,
        usage["prompt_tokens"],
        0,
        False,
        result["cost_usd"],
        latency_ms,
        trace_id,
    )
    record_metrics(provider, model, "embed", False, result["cost_usd"])
    return result
