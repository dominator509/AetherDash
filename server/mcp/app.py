"""MCP tool server — stub with tier-filtered manifest.
Full implementations: EP-201 (brain), EP-202 (LLM), EP-203 (alerts)."""

import tomllib
from pathlib import Path

from fastapi import FastAPI, HTTPException, Query
from pydantic import BaseModel

app = FastAPI(title="AETHER MCP Server", version="0.1.0")

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


@app.get("/healthz")
async def healthz():
    return {"status": "ok", "service": "mcp"}


@app.get("/tools")
async def list_tools(tier: int = Query(default=5, ge=1, le=5)):
    """List tools available at or below the given tier."""
    tools = filter_by_tier(tier)
    return {"tier": tier, "tools": tools}


@app.post("/tools/{tool_name}")
async def call_tool(tool_name: str, tier: int = Query(default=5)):
    """Stub: echo back the tool name. Real implementations in EP-201+."""
    manifest = load_manifest()
    tool = next((t for t in manifest if t["name"] == tool_name), None)
    if tool is None:
        raise HTTPException(status_code=404, detail=f"Unknown tool: {tool_name}")
    if tier < tool["tier"]:
        raise HTTPException(
            status_code=403,
            detail=f"Tool '{tool_name}' requires tier {tool['tier']}; caller has tier {tier}",
        )
    return {
        "tool": tool_name,
        "result": f"stub: {tool['description']}",
        "note": "Full implementation deferred to EP-201/EP-202/EP-203",
    }


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="127.0.0.1", port=8000)
