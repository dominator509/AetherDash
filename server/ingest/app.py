"""Ingestion fleet service: scheduler, audit, metrics, and health surfaces."""

import os
from contextlib import asynccontextmanager

import asyncpg
import structlog
from fastapi import FastAPI, HTTPException, Query
from fastapi.responses import PlainTextResponse

from server.ingest.logging_config import configure_logging
from server.ingest.metrics import render_metrics
from server.ingest.runtime import FleetRuntime, load_source_configs

configure_logging()
logger = structlog.get_logger().bind(service="ingest", plane="brain")

_pool: asyncpg.Pool | None = None
_runtime: FleetRuntime | None = None


@asynccontextmanager
async def lifespan(app: FastAPI):  # noqa: ARG001
    global _pool, _runtime  # noqa: PLW0603
    database_url = os.environ.get(
        "DATABASE_URL", "postgres://aether:aether@localhost:5432/aether"
    )
    config_path = os.environ.get("AETHER_INGEST__CONFIG_PATH")
    if not config_path:
        raise RuntimeError("AETHER_INGEST__CONFIG_PATH is required")
    configs = load_source_configs(config_path)
    if not configs:
        raise RuntimeError("at least one ingestion source must be configured")
    _pool = await asyncpg.create_pool(database_url, min_size=1, max_size=10)
    try:
        _runtime = FleetRuntime(
            _pool,
            configs,
            workers=int(os.environ.get("AETHER_INGEST__WORKERS", "4")),
        )
        await _runtime.start()
        logger.info("service_started", sources=len(configs))
        yield
    finally:
        logger.info("service_stopping")
        if _runtime is not None:
            await _runtime.close()
        await _pool.close()
        _runtime = None
        _pool = None


app = FastAPI(title="AETHER Ingestion Fleet", version="0.1.0", lifespan=lifespan)


@app.get("/healthz")
async def healthz() -> dict[str, str]:
    return {"status": "ok", "service": "ingest"}


@app.get("/readyz")
async def readyz() -> dict[str, str]:
    if _pool is None or _runtime is None or not _runtime.healthy():
        raise HTTPException(503, "ingestion runtime is not ready")
    try:
        await _pool.fetchval("SELECT 1")
    except Exception as exc:
        raise HTTPException(503, "ingestion database is unavailable") from exc
    return {"status": "ok", "service": "ingest"}


@app.get("/metrics", response_class=PlainTextResponse)
async def metrics() -> PlainTextResponse:
    if _pool is None:
        raise HTTPException(503, "ingestion database is unavailable")
    return PlainTextResponse(
        await render_metrics(_pool), media_type="text/plain; version=0.0.4"
    )


@app.get("/audit/sources")
async def source_audit(limit: int = Query(default=100, ge=1, le=500)) -> dict:
    if _pool is None:
        raise HTTPException(503, "ingestion database is unavailable")
    rows = await _pool.fetch(
        """
        SELECT object_id,source,ladder_rung,bytes,status,trace_id,created_ts
        FROM ingest_source_events ORDER BY created_ts DESC,id DESC LIMIT $1
        """,
        limit,
    )
    decisions = await _pool.fetch(
        """
        SELECT source,from_rung,to_rung,reason,approved_by,created_ts
        FROM ingest_rung_decisions ORDER BY created_ts DESC,id DESC LIMIT $1
        """,
        limit,
    )
    return {
        "events": [dict(row) for row in rows],
        "downgrades": [dict(row) for row in decisions],
    }
