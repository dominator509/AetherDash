"""Transport adapter for the canonical Rust simulator.

The Rust binary owns all fill and net-edge math.  This module only validates
the request shape, invokes that binary without a shell, and decodes its JSON
response so the MCP layer cannot drift into a second simulator implementation.
"""

from __future__ import annotations

import asyncio
import json
import os
from pathlib import Path
from typing import Any


class SimulatorUnavailableError(RuntimeError):
    """The canonical simulator could not be invoked."""


class SimulationRejectedError(ValueError):
    """The canonical simulator rejected the supplied scenario."""


def _simulator_command() -> str:
    configured = os.environ.get("AETHER_SIMULATOR_BIN")
    if configured:
        return configured
    candidate = Path(
        "target/debug/aether-simulator.exe"
        if os.name == "nt"
        else "target/debug/aether-simulator"
    )
    return str(candidate)


async def run_simulation(payload: dict[str, Any]) -> dict[str, Any]:
    """Run one scenario through the canonical Rust JSON transport."""
    if not isinstance(payload, dict) or not payload:
        raise SimulationRejectedError("sim.run requires a non-empty JSON object")

    try:
        encoded = json.dumps(payload, separators=(",", ":"), allow_nan=False).encode()
    except (TypeError, ValueError) as exc:
        raise SimulationRejectedError("sim.run payload is not valid JSON") from exc

    try:
        process = await asyncio.create_subprocess_exec(
            _simulator_command(),
            stdin=asyncio.subprocess.PIPE,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
    except OSError as exc:
        raise SimulatorUnavailableError(
            "canonical simulator binary is unavailable"
        ) from exc

    try:
        stdout, stderr = await asyncio.wait_for(
            process.communicate(encoded), timeout=15
        )
    except TimeoutError as exc:
        process.kill()
        await process.wait()
        raise SimulatorUnavailableError("canonical simulator timed out") from exc

    if process.returncode != 0:
        # The Rust CLI intentionally emits no secret-bearing input.  Do not
        # reflect stderr because dependency errors may contain local paths.
        raise SimulationRejectedError("canonical simulator rejected the scenario")

    try:
        result = json.loads(stdout)
    except json.JSONDecodeError as exc:
        raise SimulatorUnavailableError(
            "canonical simulator returned invalid JSON"
        ) from exc
    if not isinstance(result, dict):
        raise SimulatorUnavailableError(
            "canonical simulator returned an invalid response"
        )
    decomposition = result.get("decomposition")
    required_components = {
        "gross_spread",
        "fees",
        "slippage_est",
        "funding_cost",
        "gas_cost",
        "bridge_cost",
        "settlement_mismatch_discount",
        "liquidity_haircut",
        "staleness_penalty",
        "confidence_penalty",
        "net_edge",
    }
    if (
        not isinstance(decomposition, dict)
        or not required_components.issubset(decomposition)
        or not isinstance(result.get("buy_fills"), list)
        or not isinstance(result.get("sell_fills"), list)
        or not isinstance(result.get("sensitivity"), dict)
    ):
        raise SimulatorUnavailableError(
            "canonical simulator returned an incomplete response"
        )
    return result
