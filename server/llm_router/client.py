"""Thin client for the AETHER LLM Router — importable by other services.

Usage::

    from server.llm_router.client import complete

    result = await complete(
        "summarize",
        messages=[{"role": "user", "content": "..."}],
    )
"""

from __future__ import annotations

import os
import time
from typing import Any

import httpx

_AETHER_LLM_ROUTER_URL = os.environ.get("AETHER_LLM__URL", "http://127.0.0.1:8001")

_DEFAULT_TIMEOUT = 60.0
_OUTAGE_BACKOFF_SECONDS = 5.0
_unavailable_until = 0.0


def _router_available() -> bool:
    return time.monotonic() >= _unavailable_until


def _mark_router_unavailable() -> None:
    global _unavailable_until  # noqa: PLW0603
    _unavailable_until = time.monotonic() + _OUTAGE_BACKOFF_SECONDS


async def complete(
    purpose: str,
    dynamic_inputs: dict[str, Any],
    model_policy: str = "default",
    *,
    static_context_ref: str | None = None,
    rag_chunks: list[str] | None = None,
    base_url: str | None = None,
    timeout: float = _DEFAULT_TIMEOUT,
    max_tokens: int | None = None,
) -> dict[str, Any]:
    """Call the LLM Router's ``/complete`` endpoint.

    Args:
        purpose: One of ``"summarize"``, ``"extract"``, ``"embed"``, ``"chat"``.
        messages: OpenAI-style message list.
        model_policy: One of ``"default"``, ``"cheap"``, ``"local"``.
        base_url: Override the router base URL (default from env or
            ``http://localhost:8010``).
        timeout: HTTP request timeout in seconds.

    Returns:
        The router response dict with keys ``text``, ``usage``, ``model``,
        ``provider``, ``cache_hit``, ``cost_usd``.  Returns an error dict
        with ``{"text": "", "error": "<reason>", "provider": "error"}`` when
        the router is unreachable or returns a non-2xx status.
    """
    url = (base_url or _AETHER_LLM_ROUTER_URL).rstrip("/") + "/complete"

    if not _router_available():
        return {"text": "", "error": "router_unavailable", "provider": "error"}

    try:
        request_timeout = httpx.Timeout(timeout, connect=min(timeout, 0.25))
        async with httpx.AsyncClient(timeout=request_timeout) as client:
            resp = await client.post(
                url,
                json={
                    "purpose": purpose,
                    "static_context_ref": static_context_ref,
                    "dynamic_inputs": dynamic_inputs,
                    "rag_chunks": rag_chunks or [],
                    "model_policy": model_policy,
                    "max_tokens": max_tokens,
                },
            )
            resp.raise_for_status()
            return resp.json()
    except httpx.HTTPError as exc:
        _mark_router_unavailable()
        return {"text": "", "error": str(exc), "provider": "error"}


async def embed(
    text: str,
    model_policy: str = "default",
    *,
    base_url: str | None = None,
    timeout: float = 10.0,
) -> dict[str, Any]:
    """Call the router's embedding-specific endpoint."""
    url = (base_url or _AETHER_LLM_ROUTER_URL).rstrip("/") + "/embed"
    if not _router_available():
        return {"embedding": [], "error": "router_unavailable", "provider": "error"}
    try:
        request_timeout = httpx.Timeout(timeout, connect=min(timeout, 0.25))
        async with httpx.AsyncClient(timeout=request_timeout) as client:
            response = await client.post(
                url, json={"text": text, "model_policy": model_policy}
            )
            response.raise_for_status()
            return response.json()
    except httpx.HTTPError as exc:
        _mark_router_unavailable()
        return {"embedding": [], "error": str(exc), "provider": "error"}
