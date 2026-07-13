"""FastAPI application for the AETHER Brain service.

Provides REST endpoints and runs alongside the gRPC server.
"""

import logging
import os

import asyncpg
from fastapi import FastAPI, Response

logger = logging.getLogger(__name__)

app = FastAPI(
    title="AETHER Brain",
    description="Knowledge graph API, tiered recall, vault-view generator",
    version="0.1.0",
)


@app.get("/healthz")
async def healthz() -> dict[str, str]:
    """Health check — always returns OK if the process is alive."""
    return {"status": "ok", "service": "brain"}


@app.get("/readyz")
async def readyz(response: Response) -> dict[str, str]:
    """Readiness check — verifies Postgres connectivity."""
    database_url = os.environ.get(
        "DATABASE_URL", "postgres://aether:aether@localhost:5432/aether"
    )
    try:
        conn = await asyncpg.connect(database_url)
        await conn.execute("SELECT 1")
        await conn.close()
        return {"status": "ok", "postgres": "connected"}
    except Exception:
        logger.exception("readyz: postgres not reachable")
        response.status_code = 503
        return {"status": "degraded", "database": "unreachable"}
