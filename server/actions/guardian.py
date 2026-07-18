"""No-shell adapter for authenticated Wallet Guardian gRPC approvals."""

from __future__ import annotations

import asyncio
import json
import os
from pathlib import Path
from typing import Any


class GuardianApprovalRejectedError(RuntimeError):
    """The Guardian did not authoritatively approve the proposal."""


def _command() -> str:
    configured = os.environ.get("AETHER_GUARDIAN_CLIENT_BIN")
    if configured:
        return configured
    return str(
        Path(
            "target/debug/guardian-client.exe"
            if os.name == "nt"
            else "target/debug/guardian-client"
        )
    )


async def approve_guardian(payload: dict[str, Any]) -> dict[str, Any]:
    endpoint = os.environ.get("AETHER_GUARDIAN_ENDPOINT", "")
    if not endpoint.startswith(("http://127.0.0.1:", "http://localhost:")):
        raise GuardianApprovalRejectedError(
            "Guardian endpoint must be configured on loopback"
        )
    request = {"endpoint": endpoint, **payload}
    try:
        encoded = json.dumps(request, separators=(",", ":"), allow_nan=False).encode()
    except (TypeError, ValueError) as exc:
        raise GuardianApprovalRejectedError("Guardian request is invalid") from exc
    try:
        process = await asyncio.create_subprocess_exec(
            _command(),
            stdin=asyncio.subprocess.PIPE,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
    except OSError as exc:
        raise GuardianApprovalRejectedError("Guardian client is unavailable") from exc
    try:
        stdout, _stderr = await asyncio.wait_for(
            process.communicate(encoded), timeout=15
        )
    except TimeoutError as exc:
        process.kill()
        await process.wait()
        raise GuardianApprovalRejectedError("Guardian approval timed out") from exc
    if process.returncode != 0:
        raise GuardianApprovalRejectedError("Guardian rejected the approval")
    try:
        result = json.loads(stdout)
    except json.JSONDecodeError as exc:
        raise GuardianApprovalRejectedError("Guardian returned invalid JSON") from exc
    if not isinstance(result, dict) or result.get("status") != "completed":
        raise GuardianApprovalRejectedError("Guardian did not confirm completion")
    return result
