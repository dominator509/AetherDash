"""FastAPI application for the AETHER LLM Router."""

from __future__ import annotations

import logging
from contextlib import asynccontextmanager
from typing import Any

from fastapi import FastAPI, HTTPException
from fastapi.responses import PlainTextResponse
from pydantic import BaseModel, Field

from server.llm_router.cache import _close_redis, _get_redis
from server.llm_router.fallback import (
    complete_with_fallback,
    get_fallback_metrics_text,
)
from server.llm_router.metrics import get_metrics_text
from server.llm_router.prompt.builder import build_prompt
from server.llm_router.router import embed as router_embed

logger = logging.getLogger("aether.llm_router")

# ---------------------------------------------------------------------------
# Request / Response models
# ---------------------------------------------------------------------------


class CompleteRequest(BaseModel):
    purpose: str
    static_context_ref: str | None = None
    dynamic_inputs: dict[str, Any] = Field(default_factory=dict)
    rag_chunks: list[str] = Field(default_factory=list)
    model_policy: str = "default"


class EmbedRequest(BaseModel):
    text: str
    model_policy: str = "default"


class HealthResponse(BaseModel):
    status: str
    service: str


# ---------------------------------------------------------------------------
# Application lifespan
# ---------------------------------------------------------------------------


@asynccontextmanager
async def lifespan(app: FastAPI):
    """Startup/shutdown lifecycle."""
    logger.info("llm_router starting")
    yield
    await _close_redis()
    logger.info("llm_router shutting down")


# ---------------------------------------------------------------------------
# App instance
# ---------------------------------------------------------------------------

app = FastAPI(
    title="AETHER LLM Router",
    version="0.1.0",
    lifespan=lifespan,
)


# ---------------------------------------------------------------------------
# Routes
# ---------------------------------------------------------------------------


@app.post("/complete")
async def complete_endpoint(body: CompleteRequest) -> dict[str, Any]:
    """Route a completion through the configured provider."""
    try:
        assembly = await build_prompt(
            purpose=body.purpose,
            static_context_ref=body.static_context_ref,
            dynamic_inputs=body.dynamic_inputs,
            rag_chunks=body.rag_chunks,
        )
        result = await complete_with_fallback(
            purpose=body.purpose,
            messages=assembly.messages,
            model_policy=body.model_policy,
        )
    except ValueError as exc:
        raise HTTPException(status_code=422, detail=str(exc)) from exc

    return result


@app.post("/embed")
async def embed_endpoint(body: EmbedRequest) -> dict[str, Any]:
    """Route an embedding through the embedding-specific LiteLLM API."""
    try:
        return await router_embed(body.text, body.model_policy)
    except ValueError as exc:
        raise HTTPException(status_code=422, detail=str(exc)) from exc


@app.get("/healthz")
async def healthz() -> HealthResponse:
    """Basic health check."""
    return HealthResponse(status="ok", service="llm_router")


@app.get("/readyz")
async def readyz() -> dict[str, Any]:
    """Readiness check — verify Redis and basic provider connectivity."""
    checks: dict[str, Any] = {
        "redis": "unknown",
        "providers": {},
    }

    # Basic Redis connectivity check
    try:
        redis_client = await _get_redis()
        checks["redis"] = "ok" if redis_client is not None else "unavailable"
    except Exception:
        checks["redis"] = "unavailable"

    # Provider key presence check (not actual connectivity)
    from server.llm_router.config import get_api_keys

    for prov, key in get_api_keys().items():
        checks["providers"][prov] = "configured" if key else "missing"

    overall = all(v == "ok" for k, v in checks.items() if k in ("redis",))
    return {
        "status": "ok" if overall else "degraded",
        "checks": checks,
    }


@app.get("/metrics", response_class=PlainTextResponse)
async def metrics() -> str:
    """Prometheus-format metrics endpoint."""
    return get_metrics_text() + get_fallback_metrics_text()
