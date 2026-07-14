"""FastAPI app for the AETHER Agentic Inbox service.

Mounts webhook receivers and provides health-check / reprocess endpoints.

Endpoints
---------
- GET /healthz              — liveness check
- GET /readyz               — readiness check
- POST /webhooks/gmail      — Gmail Pub/Sub push notifications
- POST /webhooks/msgraph    — MS Graph subscription notifications
- POST /inbox/reprocess     — reprocess a stored object (Tier 3+)
"""

import asyncio
import logging
from contextlib import asynccontextmanager

from fastapi import FastAPI, Request
from fastapi.responses import JSONResponse

from server.inbox.processing import worker_loop
from server.inbox.webhooks import gmail, msgraph
from server.mcp.auth import AuthError, authenticate, close_pool, init_pool

logger = logging.getLogger(__name__)

_ready = False
_worker_task: asyncio.Task | None = None


@asynccontextmanager
async def lifespan(app: FastAPI):  # noqa: ARG001
    """Simple lifespan — marks the service ready after startup."""
    logger.info("inbox service starting")
    global _ready, _worker_task  # noqa: PLW0603
    await init_pool()
    stop = asyncio.Event()
    worker = asyncio.create_task(worker_loop(stop))
    _worker_task = worker
    _ready = True
    yield
    _ready = False
    stop.set()
    await worker
    _worker_task = None
    await close_pool()
    logger.info("inbox service shutting down")


app = FastAPI(
    title="AETHER Inbox",
    version="0.1.0",
    lifespan=lifespan,
)

# --- Mount webhook routers ---
app.include_router(gmail.router)
app.include_router(msgraph.router)


# --- Health ---


@app.get("/healthz")
async def healthz() -> dict:
    """Liveness check."""
    return {"status": "ok", "service": "inbox"}


@app.get("/readyz")
async def readyz() -> dict:
    """Readiness check."""
    worker_ok = _worker_task is not None and not _worker_task.done()
    status = "ok" if _ready and worker_ok else "degraded"
    return {"status": status, "service": "inbox"}


# --- Reprocess ---


@app.post("/inbox/reprocess", response_model=None)
async def reprocess_object(request: Request):
    """Reprocess a stored raw object through the Brain pipeline.

    Tier 3+ gated through the shared authenticated session/grant path.
    Looks up the object by its brain object ID and re-submits the
    original raw bytes to Brain.Store.

    Request body: ``{"object_id": "..."}``
    """
    from server.brain import service as brain_service
    from server.brain import storage as brain_storage

    try:
        session = await authenticate(request.headers.get("Authorization"))
    except AuthError:
        return JSONResponse({"error": "Unauthenticated"}, status_code=401)
    if session.tier < 3:
        return JSONResponse({"error": "Tier 3+ required"}, status_code=403)

    # --- Parse body ---
    try:
        body = await request.json()
    except Exception:
        return JSONResponse({"error": "Invalid JSON body"}, status_code=400)

    object_id = (body or {}).get("object_id")
    if not object_id:
        return JSONResponse({"error": "Missing object_id"}, status_code=400)

    # --- Look up the object ---
    brain_obj = await brain_service.get_by_id(object_id)
    if brain_obj is None:
        return JSONResponse(
            {"error": "Object not found", "object_id": object_id},
            status_code=404,
        )

    if brain_obj.raw_ref is None:
        return JSONResponse(
            {"error": "Object has no raw content", "object_id": object_id},
            status_code=400,
        )

    # --- Fetch raw bytes from MinIO ---
    try:
        brain_storage.get_raw(brain_obj.raw_ref)
    except Exception as exc:
        logger.error("Failed to fetch raw bytes for %s: %s", object_id, exc)
        return JSONResponse({"error": "Failed to fetch raw content"}, status_code=500)

    # Re-run the existing object through the pipeline. Creating a new object
    # would break provenance and content-hash idempotency.
    await brain_service.reprocess_object(object_id)

    return {
        "status": "ok",
        "original_object_id": object_id,
        "object_id": object_id,
    }


# Shortcut: if run directly, serve via uvicorn
if __name__ == "__main__":
    import os

    import uvicorn

    bind = os.environ.get("AETHER_INBOX__BIND", "127.0.0.1:8003")
    host, port = bind.rsplit(":", 1)
    uvicorn.run("server.inbox.app:app", host=host, port=int(port), reload=True)
