"""MCP tool server — authenticated with tier-filtered manifest.
Full implementations: EP-201 (brain), EP-202 (LLM), EP-203 (alerts)."""

import tomllib
from pathlib import Path

from fastapi import FastAPI, Header, HTTPException
from fastapi.exceptions import RequestValidationError
from fastapi.responses import JSONResponse
from pydantic import BaseModel

from auth import AuthError, authenticate
from error_envelope import ErrorCode, new_error_envelope

app = FastAPI(title="AETHER MCP Server", version="0.1.0")


@app.exception_handler(HTTPException)
async def http_exception_handler(request, exc: HTTPException):
    """Return ErrorEnvelope dicts as top-level body (no 'detail' wrapper)."""
    if isinstance(exc.detail, dict):
        return JSONResponse(status_code=exc.status_code, content=exc.detail)
    return JSONResponse(status_code=exc.status_code, content={"detail": exc.detail})


@app.exception_handler(RequestValidationError)
async def validation_exception_handler(request, exc: RequestValidationError):
    """Convert FastAPI validation errors into ErrorEnvelope format."""
    return JSONResponse(
        status_code=400,
        content=new_error_envelope(
            code=ErrorCode.invalid_argument,
            message="Invalid request: " + str(exc.errors()),
        ),
    )


@app.exception_handler(Exception)
async def unexpected_exception_handler(request, exc: Exception):
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


def load_manifest() -> list[dict]:
    with open(MANIFEST_PATH, "rb") as f:
        data = tomllib.load(f)
    return data.get("tools", [])


def filter_by_tier(tier: int) -> list[ToolInfo]:
    tools = load_manifest()
    return [
        ToolInfo(name=t["name"], tier=t["tier"], description=t["description"])
        for t in tools
        if t["tier"] <= tier
    ]


def _abort(status: int, code: ErrorCode, message: str) -> None:
    """Raise an HTTPException with an ErrorEnvelope body."""
    raise HTTPException(
        status_code=status,
        detail=new_error_envelope(code, message),
    )


@app.get("/healthz")
async def healthz():
    return {"status": "ok", "service": "mcp"}


@app.get("/tools")
async def list_tools(authorization: str | None = Header(None)):
    """List tools available to the authenticated session's tier."""
    try:
        session = authenticate(authorization)
    except AuthError as e:
        _abort(401, ErrorCode.unauthenticated, str(e))

    tools = filter_by_tier(session.tier)
    return {"tier": session.tier, "tools": tools}


@app.post("/tools/{tool_name}")
async def call_tool(tool_name: str, authorization: str | None = Header(None)):
    """Stub: echo back the tool name. Real implementations in EP-201+."""
    try:
        session = authenticate(authorization)
    except AuthError as e:
        _abort(401, ErrorCode.unauthenticated, str(e))

    manifest = load_manifest()
    tool = next((t for t in manifest if t["name"] == tool_name), None)
    if tool is None:
        _abort(404, ErrorCode.not_found, f"Unknown tool: {tool_name}")

    if session.tier < tool["tier"]:
        _abort(
            403,
            ErrorCode.permission_denied,
            f"Tool '{tool_name}' requires tier {tool['tier']}; caller has tier {session.tier}",
        )

    return {
        "tool": tool_name,
        "result": f"stub: {tool['description']}",
        "note": "Full implementation deferred to EP-201/EP-202/EP-203",
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="127.0.0.1", port=8000)
