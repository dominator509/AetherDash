from __future__ import annotations

import json
from typing import Any

import pytest

from server.actions import app as action_app
from server.actions.executor import PaperExecutionRejectedError, execute_paper
from server.actions.guardian import GuardianApprovalRejectedError, approve_guardian
from server.actions.service import (
    ActionRejectedError,
    ActionService,
    _scope_set,
    _stable_ulid,
)


def test_stable_ulid_is_deterministic_and_canonical() -> None:
    first = _stable_ulid("caps:7")
    assert first == _stable_ulid("caps:7")
    assert len(first) == 26
    assert set(first) <= set("0123456789ABCDEFGHJKMNPQRSTVWXYZ")


def test_scope_set_distinguishes_unrestricted_from_allowlist() -> None:
    assert _scope_set({}) == ([], False)
    assert _scope_set({"allowed": ["orders.submit_paper"]}) == (
        ["orders.submit_paper"],
        True,
    )
    with pytest.raises(ActionRejectedError):
        _scope_set({"allowed": "orders.submit_paper"})


class _Process:
    def __init__(self, returncode: int, body: dict[str, Any]) -> None:
        self.returncode = returncode
        self._body = body
        self.received = b""

    async def communicate(self, payload: bytes) -> tuple[bytes, bytes]:
        self.received = payload
        return json.dumps(self._body).encode(), b"sensitive detail must not escape"

    def kill(self) -> None:
        self.returncode = -1

    async def wait(self) -> None:
        return None


@pytest.mark.asyncio
async def test_executor_accepts_only_explicit_completion(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    completed = _Process(0, {"status": "completed", "order_id": "order"})

    async def spawn(*args: Any, **kwargs: Any) -> _Process:  # noqa: ARG001
        return completed

    monkeypatch.setattr("asyncio.create_subprocess_exec", spawn)
    assert (await execute_paper({"intent": {"paper": True}}))["status"] == "completed"
    assert json.loads(completed.received) == {"intent": {"paper": True}}

    queued = _Process(0, {"status": "queued"})

    async def spawn_queued(*args: Any, **kwargs: Any) -> _Process:  # noqa: ARG001
        return queued

    monkeypatch.setattr("asyncio.create_subprocess_exec", spawn_queued)
    with pytest.raises(PaperExecutionRejectedError, match="did not confirm"):
        await execute_paper({"intent": {"paper": True}})


@pytest.mark.asyncio
async def test_guardian_adapter_uses_stdin_and_requires_explicit_completion(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    completed = _Process(0, {"status": "completed", "proposal_id": "proposal"})
    spawned_args: tuple[Any, ...] = ()

    async def spawn(*args: Any, **kwargs: Any) -> _Process:  # noqa: ARG001
        nonlocal spawned_args
        spawned_args = args
        return completed

    monkeypatch.setenv("AETHER_GUARDIAN_ENDPOINT", "http://127.0.0.1:50053")
    monkeypatch.setenv("AETHER_GUARDIAN_CLIENT_BIN", "guardian-client")
    monkeypatch.setattr("asyncio.create_subprocess_exec", spawn)
    payload = {
        "proposal_id": "01ARZ3NDEKTSV4RRFFQ69G5FAV",
        "session_token": "session-secret",
        "reference": "r" * 43,
        "totp": "123456",
    }
    assert (await approve_guardian(payload))["status"] == "completed"
    assert spawned_args == ("guardian-client",)
    assert json.loads(completed.received) == {
        "endpoint": "http://127.0.0.1:50053",
        **payload,
    }

    incomplete = _Process(0, {"status": "approved"})

    async def spawn_incomplete(*args: Any, **kwargs: Any) -> _Process:  # noqa: ARG001
        return incomplete

    monkeypatch.setattr("asyncio.create_subprocess_exec", spawn_incomplete)
    with pytest.raises(GuardianApprovalRejectedError, match="did not confirm"):
        await approve_guardian(payload)


@pytest.mark.asyncio
async def test_guardian_http_endpoint_requires_human_bearer_and_delegates(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    request = action_app.GuardianApprovalRequest(
        reference="r" * 43,
        totp="123456",
    )
    with pytest.raises(action_app.HTTPException) as missing:
        await action_app.guardian_approve(request, None)
    assert missing.value.status_code == 401

    seen: dict[str, Any] = {}

    async def approve(payload: dict[str, Any]) -> dict[str, Any]:
        seen.update(payload)
        return {"status": "completed"}

    class GuardianPool:
        async def fetchval(self, query: str, token_hash: str) -> str:
            assert "action='guardian'" in query
            assert len(token_hash) == 64
            return "01ARZ3NDEKTSV4RRFFQ69G5FAV"

    monkeypatch.setattr(action_app, "_pool", GuardianPool())
    monkeypatch.setattr(action_app, "approve_guardian", approve)
    result = await action_app.guardian_approve(request, "Bearer session-secret")
    assert result == {"status": "completed"}
    assert seen == {
        "proposal_id": "01ARZ3NDEKTSV4RRFFQ69G5FAV",
        "session_token": "session-secret",
        "reference": "r" * 43,
        "totp": "123456",
    }


class _Connection:
    def __init__(self, row: dict[str, Any] | None) -> None:
        self.row = row
        self.query = ""

    async def fetchrow(self, query: str, *args: Any) -> dict[str, Any] | None:  # noqa: ARG002
        self.query = query
        return self.row


@pytest.mark.asyncio
async def test_grant_lookup_is_current_and_scope_bound() -> None:
    row = {
        "id": "grant",
        "tier": 3,
        "scopes": {"allowed": ["orders.submit_paper"]},
    }
    conn = _Connection(row)
    service = ActionService(None)  # type: ignore[arg-type]
    assert (
        await service._grant(conn, "actor", 3, "orders.submit_paper")  # type: ignore[arg-type]
    ) == row
    assert "revoked_ts IS NULL" in conn.query
    assert "expires_ts > now()" in conn.query
    with pytest.raises(ActionRejectedError, match="scope"):
        await service._grant(conn, "actor", 3, "sim.run")  # type: ignore[arg-type]


def test_internal_service_auth_fails_closed(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("AETHER_ACTIONS_SERVICE_TOKEN", "expected")
    monkeypatch.setattr(action_app, "_pool", object())
    with pytest.raises(action_app.HTTPException) as missing:
        action_app._service(None)
    assert missing.value.status_code == 401
    assert isinstance(action_app._service("Bearer expected"), ActionService)


class _Transaction:
    async def __aenter__(self) -> None:
        return None

    async def __aexit__(self, *args: Any) -> None:
        return None


class _LifecycleConnection:
    def __init__(self) -> None:
        self.executed = False

    def transaction(self) -> _Transaction:
        return _Transaction()

    async def fetchval(self, query: str, *args: Any) -> str:  # noqa: ARG002
        assert "FOR UPDATE" in query
        return "accepted"

    async def execute(self, query: str, *args: Any) -> None:  # noqa: ARG002
        if (
            "INSERT INTO opportunity_events" in query
            and "'accepted','executed'" in query
        ):
            self.executed = True


class _Acquire:
    def __init__(self, connection: _LifecycleConnection) -> None:
        self._connection = connection

    async def __aenter__(self) -> _LifecycleConnection:
        return self._connection

    async def __aexit__(self, *args: Any) -> None:
        return None


class _Pool:
    def __init__(self) -> None:
        self.connection = _LifecycleConnection()

    def acquire(self) -> _Acquire:
        return _Acquire(self.connection)


class _LifecycleService(ActionService):
    async def _grant(self, *args: Any, **kwargs: Any) -> dict[str, Any]:
        return {"tier": 5}

    async def _opportunity(self, *args: Any, **kwargs: Any) -> dict[str, Any]:
        return {"id": "01ARZ3NDEKTSV4RRFFQ69G5FAV", "state": "accepted"}

    async def _paper_requests(self, *args: Any, **kwargs: Any) -> list[dict[str, Any]]:
        return [{"leg": 1}, {"leg": 2}]


@pytest.mark.asyncio
async def test_multileg_lifecycle_completes_only_after_every_order() -> None:
    pool = _Pool()
    seen: list[int] = []

    async def complete(request: dict[str, Any]) -> dict[str, Any]:
        seen.append(request["leg"])
        return {
            "status": "completed",
            "order_id": f"order-{request['leg']}",
            "replayed": False,
        }

    service = _LifecycleService(pool, complete)  # type: ignore[arg-type]
    result = await service.execute_paper(
        "01ARZ3NDEKTSV4RRFFQ69G5FAV", "01ARZ3NDEKTSV4RRFFQ69G5FBF", None
    )
    assert seen == [1, 2]
    assert pool.connection.executed
    assert len(result["orders"]) == 2


@pytest.mark.asyncio
async def test_multileg_failure_never_marks_opportunity_executed() -> None:
    pool = _Pool()

    async def fail_second(request: dict[str, Any]) -> dict[str, Any]:
        if request["leg"] == 2:
            raise PaperExecutionRejectedError("second leg failed")
        return {"status": "completed", "order_id": "order-1", "replayed": False}

    service = _LifecycleService(pool, fail_second)  # type: ignore[arg-type]
    with pytest.raises(PaperExecutionRejectedError):
        await service.execute_paper(
            "01ARZ3NDEKTSV4RRFFQ69G5FAV", "01ARZ3NDEKTSV4RRFFQ69G5FBF", None
        )
    assert not pool.connection.executed
