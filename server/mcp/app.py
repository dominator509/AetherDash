"""MCP tool server — authenticated with tier-filtered manifest.
Full implementations: EP-201 (brain), EP-202 (LLM), EP-203 (alerts)."""

from __future__ import annotations

import tomllib
from collections.abc import AsyncIterator
from contextlib import asynccontextmanager
from pathlib import Path
from typing import Any

from auth import (
    AuthError,
    PermissionDeniedError,
    authenticate,
    close_pool,
    init_pool,
)
from authz import Verdict, emit_decision, evaluate_tool
from confirmation import ConfirmationStore
from error_envelope import ErrorCode, new_error_envelope
from fastapi import FastAPI, Header, HTTPException
from fastapi.exceptions import RequestValidationError
from fastapi.responses import JSONResponse
from pydantic import BaseModel
from simulator import SimulationRejectedError, SimulatorUnavailableError, run_simulation

from server.swarm.orchestrator import (
    ProgressEvent,
    SwarmNoEvidenceError,
    SwarmOrchestrator,
    SwarmRequest,
)


@asynccontextmanager
async def lifespan(app: FastAPI) -> AsyncIterator[None]:
    """Manage the asyncpg connection pool lifecycle."""
    await init_pool()
    yield
    await close_pool()


app = FastAPI(title="AETHER MCP Server", version="0.1.0", lifespan=lifespan)
_confirmations = ConfirmationStore()


@app.exception_handler(HTTPException)
async def http_exception_handler(request: object, exc: HTTPException) -> JSONResponse:
    """Return ErrorEnvelope dicts as top-level body (no 'detail' wrapper)."""
    if isinstance(exc.detail, dict):
        return JSONResponse(status_code=exc.status_code, content=exc.detail)
    return JSONResponse(status_code=exc.status_code, content={"detail": exc.detail})


@app.exception_handler(RequestValidationError)
async def validation_exception_handler(
    request: object, exc: RequestValidationError
) -> JSONResponse:
    """Convert FastAPI validation errors into ErrorEnvelope format.
    Returns only field/location metadata — never raw input values."""
    sanitized: list[dict[str, object]] = [
        {
            "loc": list(e.get("loc", [])),
            "type": e.get("type", ""),
        }
        for e in exc.errors()
    ]
    return JSONResponse(
        status_code=400,
        content=new_error_envelope(
            code=ErrorCode.invalid_argument,
            message="Invalid request",
            details=str(sanitized),
        ),
    )


@app.exception_handler(Exception)
async def unexpected_exception_handler(request: object, exc: Exception) -> JSONResponse:
    """Catch-all: convert unexpected errors into ErrorEnvelope."""
    return JSONResponse(
        status_code=500,
        content=new_error_envelope(
            code=ErrorCode.internal,
            message="Internal server error",
        ),
    )


MANIFEST_PATH = Path(__file__).parent / "manifest.toml"


class ToolInfo(BaseModel):
    name: str
    tier: int
    description: str


def load_manifest() -> list[dict[str, Any]]:
    with open(MANIFEST_PATH, "rb") as f:
        data: dict[str, Any] = tomllib.load(f)
    return data.get("tools", [])  # type: ignore[no-any-return]


def filter_for_session(session: Any) -> list[ToolInfo]:
    """Filter inventory through the same decision surface used for calls."""
    filtered: list[ToolInfo] = []
    for tool in load_manifest():
        decision = evaluate_tool(session, tool)
        emit_decision(session, str(tool["name"]), decision)
        # Confirmation and step-up tools remain visible; the model needs their
        # schemas to initiate the human-gated flow. Only hard denials disappear.
        if decision.verdict is Verdict.deny:
            continue
        filtered.append(
            ToolInfo(
                name=tool["name"],
                tier=tool["tier"],
                description=tool["description"],
            )
        )
    return filtered


def _abort(
    status: int, code: ErrorCode, message: str, *, details: str | None = None
) -> None:
    """Raise an HTTPException with an ErrorEnvelope body."""
    raise HTTPException(
        status_code=status,
        detail=new_error_envelope(code, message, details),
    )


@app.get("/healthz")
async def healthz() -> dict[str, str]:
    return {"status": "ok", "service": "mcp"}


@app.get("/tools")
async def list_tools(authorization: str | None = Header(None)) -> Any:
    """List tools available to the authenticated session's tier and scopes."""
    try:
        session = await authenticate(authorization)
    except PermissionDeniedError as e:
        _abort(403, ErrorCode.permission_denied, str(e))
    except AuthError as e:
        _abort(401, ErrorCode.unauthenticated, str(e))

    tools = filter_for_session(session)
    return {
        "tier": session.tier,
        "tools": tools,
        "scopes": session.scopes,
    }


@app.post("/tools/{tool_name}")
async def call_tool(
    tool_name: str,
    payload: dict[str, Any] | None = None,
    authorization: str | None = Header(None),
) -> Any:
    """Invoke an authorized tool, using a concrete handler when available."""
    try:
        session = await authenticate(authorization)
    except PermissionDeniedError as e:
        _abort(403, ErrorCode.permission_denied, str(e))
    except AuthError as e:
        _abort(401, ErrorCode.unauthenticated, str(e))

    manifest = load_manifest()
    tool = next((t for t in manifest if t["name"] == tool_name), None)
    if tool is None:
        _abort(404, ErrorCode.not_found, f"Unknown tool: {tool_name}")
    assert tool is not None  # type narrowing after _abort

    body = dict(payload or {})
    confirmation_ref = body.pop("confirmation_ref", None)
    confirmed = False
    if isinstance(confirmation_ref, str):
        confirmed = await _confirmations.consume(
            confirmation_ref,
            actor_id=session.actor_id,
            tool_name=tool_name,
            payload=body,
        )
        if not confirmed:
            _abort(
                412,
                ErrorCode.failed_precondition,
                "Confirmation is invalid, expired, consumed, or payload-mismatched",
            )

    decision = evaluate_tool(session, tool, confirmed=confirmed)
    emit_decision(session, tool_name, decision)
    if decision.verdict is Verdict.deny:
        _abort(
            403,
            ErrorCode.permission_denied,
            "Tool is not permitted by the current grant",
        )
    if decision.verdict is Verdict.confirm_required:
        ref_id = await _confirmations.issue(
            actor_id=session.actor_id,
            tool_name=tool_name,
            payload=body,
        )
        _abort(
            412,
            ErrorCode.failed_precondition,
            "Human confirmation is required",
            details=f"confirmation_ref={ref_id}",
        )
    if decision.verdict is Verdict.step_up_required:
        _abort(
            412,
            ErrorCode.failed_precondition,
            "Fresh step-up authentication is required",
        )

    if tool_name == "sim.run":
        try:
            return {"tool": tool_name, "result": await run_simulation(payload or {})}
        except SimulationRejectedError as exc:
            _abort(400, ErrorCode.invalid_argument, str(exc))
        except SimulatorUnavailableError:
            _abort(503, ErrorCode.unavailable, "Canonical simulator is unavailable")

    if tool_name == "swarm.launch":
        try:
            request = SwarmRequest.model_validate(body)
            progress_events: list[dict[str, Any]] = []

            async def collect_progress(event: ProgressEvent) -> None:
                progress_events.append(event.model_dump(mode="json"))

            packet = await SwarmOrchestrator().launch(
                request, progress=collect_progress
            )
            return {
                "tool": tool_name,
                "progress": progress_events,
                "result": packet.model_dump(mode="json"),
            }
        except ValueError as exc:
            _abort(400, ErrorCode.invalid_argument, str(exc))
        except SwarmNoEvidenceError:
            _abort(
                422,
                ErrorCode.failed_precondition,
                "No cited Brain evidence was recalled",
            )

    return {
        "tool": tool_name,
        "result": f"stub: {tool['description']}",
        "note": "Full implementation deferred to EP-201/EP-202/EP-203",
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="127.0.0.1", port=8000)
