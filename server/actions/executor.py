"""No-shell transport for the canonical Rust paper-order router."""

from __future__ import annotations

import asyncio
import json
import os
from pathlib import Path
from typing import Any


class PaperExecutionRejectedError(RuntimeError):
    """The authoritative router did not complete the paper order."""


def _command() -> str:
    configured = os.environ.get("AETHER_PAPER_EXECUTOR_BIN")
    if configured:
        return configured
    return str(
        Path(
            "target/debug/aether-execute-paper.exe"
            if os.name == "nt"
            else "target/debug/aether-execute-paper"
        )
    )


async def execute_paper(payload: dict[str, Any]) -> dict[str, Any]:
    """Execute one fully snapshotted request through the Rust router."""
    try:
        encoded = json.dumps(payload, separators=(",", ":"), allow_nan=False).encode()
    except (TypeError, ValueError) as exc:
        raise PaperExecutionRejectedError("paper request is not valid JSON") from exc
    try:
        process = await asyncio.create_subprocess_exec(
            _command(),
            stdin=asyncio.subprocess.PIPE,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
    except OSError as exc:
        raise PaperExecutionRejectedError("paper router is unavailable") from exc
    try:
        stdout, _stderr = await asyncio.wait_for(
            process.communicate(encoded), timeout=20
        )
    except TimeoutError as exc:
        process.kill()
        await process.wait()
        raise PaperExecutionRejectedError("paper router timed out") from exc
    if process.returncode != 0:
        raise PaperExecutionRejectedError("paper router rejected the request")
    try:
        result = json.loads(stdout)
    except json.JSONDecodeError as exc:
        raise PaperExecutionRejectedError("paper router returned invalid JSON") from exc
    if not isinstance(result, dict) or result.get("status") != "completed":
        raise PaperExecutionRejectedError("paper router did not confirm completion")
    return result
