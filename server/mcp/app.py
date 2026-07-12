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
from error_envelope import ErrorCode, new_error_envelope
from fastapi import FastAPI, Header, HTTPException
from fastapi.exceptions import RequestValidationError
from fastapi.responses import JSONResponse
from pydantic import BaseModel


@asynccontextmanager
async def lifespan(app: FastAPI) -> AsyncIterator[None]:
    """Manage the asyncpg connection pool lifecycle."""
    await init_pool()
    yield
    await close_pool()


app = FastAPI(title="AETHER MCP Server", version="0.1.0", lifespan=lifespan)


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


def filter_by_tier(
    tier: int,
    scopes: dict[str, Any] | None = None,
) -> list[ToolInfo]:
    tools = load_manifest()
    filtered = []
    for t in tools:
        if t["tier"] <= tier:
            # Apply scope filtering if present
            if scopes:
                allowed = scopes.get("allowed")
                if allowed is not None and t["name"] not in allowed:
                    continue
            filtered.append(
                ToolInfo(name=t["name"], tier=t["tier"], description=t["description"])
            )
    return filtered


def _abort(status: int, code: ErrorCode, message: str) -> None:
    """Raise an HTTPException with an ErrorEnvelope body."""
    raise HTTPException(
        status_code=status,
        detail=new_error_envelope(code, message),
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

    tools = filter_by_tier(session.tier, session.scopes)
    return {
        "tier": session.tier,
        "tools": tools,
        "scopes": session.scopes,
    }


@app.post("/tools/{tool_name}")
async def call_tool(tool_name: str, authorization: str | None = Header(None)) -> Any:
    """Stub: echo back the tool name. Real implementations in EP-201+."""
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

    if session.tier < tool["tier"]:
        _abort(
            403,
            ErrorCode.permission_denied,
            f"Tool '{tool_name}' requires tier {tool['tier']}; caller has tier {session.tier}",
        )

    # Check grant scopes
    scopes = session.scopes or {}
    allowed = scopes.get("allowed")
    if allowed is not None and tool_name not in allowed:
        _abort(
            403,
            ErrorCode.permission_denied,
            f"Tool '{tool_name}' not in grant scopes",
        )

    return {
        "tool": tool_name,
        "result": f"stub: {tool['description']}",
        "note": "Full implementation deferred to EP-201/EP-202/EP-203",
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="127.0.0.1", port=8000)
