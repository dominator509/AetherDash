"""Authenticated authoritative action-effect API for alert callbacks."""

from __future__ import annotations

import hashlib
import hmac
import json
import os
from collections.abc import AsyncIterator
from contextlib import asynccontextmanager

import asyncpg  # type: ignore[import-untyped]
from fastapi import FastAPI, Header, HTTPException
from pydantic import BaseModel, Field, SecretStr

from server.actions.guardian import (
    GuardianApprovalRejectedError,
    approve_guardian,
)
from server.actions.service import ActionRejectedError, ActionService

_pool: asyncpg.Pool | None = None


async def _init_connection(conn: asyncpg.Connection) -> None:
    """Decode JSON values consistently instead of relying on driver defaults."""
    for type_name in ("json", "jsonb"):
        await conn.set_type_codec(
            type_name,
            schema="pg_catalog",
            encoder=json.dumps,
            decoder=json.loads,
        )


class ActionRequest(BaseModel):
    opportunity_id: str = Field(min_length=26, max_length=26)
    actor_id: str = Field(min_length=26, max_length=26)
    approval_id: str | None = Field(default=None, min_length=26, max_length=26)


class GuardianApprovalRequest(BaseModel):
    reference: SecretStr
    totp: SecretStr


@asynccontextmanager
async def lifespan(app: FastAPI) -> AsyncIterator[None]:  # noqa: ARG001
    global _pool  # noqa: PLW0603
    _pool = await asyncpg.create_pool(
        os.environ["DATABASE_URL"], min_size=1, max_size=5, init=_init_connection
    )
    yield
    await _pool.close()
    _pool = None


app = FastAPI(title="AETHER Actions", version="0.1.0", lifespan=lifespan)


def _service(authorization: str | None) -> ActionService:
    expected = os.environ.get("AETHER_ACTIONS_SERVICE_TOKEN", "")
    supplied = authorization.removeprefix("Bearer ") if authorization else ""
    if not expected or not hmac.compare_digest(expected, supplied):
        raise HTTPException(401, "invalid internal service authentication")
    if _pool is None:
        raise HTTPException(503, "action service is not ready")
    return ActionService(_pool)


async def _run(
    operation: str, request: ActionRequest, authorization: str | None
) -> dict:
    service = _service(authorization)
    try:
        if operation == "simulate":
            return await service.simulate(request.opportunity_id, request.actor_id)
        if operation == "ignore":
            return await service.ignore(request.opportunity_id, request.actor_id)
        if operation == "execute-paper":
            return await service.execute_paper(
                request.opportunity_id, request.actor_id, request.approval_id
            )
    except ActionRejectedError as exc:
        raise HTTPException(412, str(exc)) from exc
    except Exception:
        raise HTTPException(503, "authoritative action could not complete") from None
    raise HTTPException(404, "unknown action")


@app.get("/healthz")
async def healthz() -> dict[str, str]:
    return {"status": "ok", "service": "actions"}


@app.post("/v1/actions/{operation}")
async def action(
    operation: str,
    request: ActionRequest,
    authorization: str | None = Header(None),
) -> dict:
    return await _run(operation, request, authorization)


@app.post("/v1/guardian/approve")
async def guardian_approve(
    request: GuardianApprovalRequest,
    authorization: str | None = Header(None),
) -> dict:
    """Complete Guardian step-up through its independent gRPC boundary."""
    if not authorization or not authorization.startswith("Bearer "):
        raise HTTPException(401, "human session authentication is required")
    session_token = authorization.removeprefix("Bearer ").strip()
    if not session_token:
        raise HTTPException(401, "human session authentication is required")
    if _pool is None:
        raise HTTPException(503, "action service is not ready")
    reference = request.reference.get_secret_value()
    try:
        proposal_id = await _pool.fetchval(
            """SELECT target_id FROM approval_references
               WHERE token_hash=$1 AND action='guardian' AND status='pending'
                 AND expires_ts>now()""",
            hashlib.sha256(reference.encode()).hexdigest(),
        )
    except Exception:
        raise HTTPException(503, "Guardian approval lookup is unavailable") from None
    if proposal_id is None:
        raise HTTPException(412, "Guardian approval reference is invalid")
    try:
        return await approve_guardian(
            {
                "proposal_id": proposal_id,
                "session_token": session_token,
                "reference": reference,
                "totp": request.totp.get_secret_value(),
            }
        )
    except GuardianApprovalRejectedError as exc:
        raise HTTPException(412, str(exc)) from exc
