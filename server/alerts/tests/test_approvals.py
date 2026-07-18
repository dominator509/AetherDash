import inspect
from datetime import UTC, datetime, timedelta

import pytest
from starlette.requests import Request

from connectors.comms.twilio.webhook import expected_signature
from server.alerts.approvals import (
    ApprovalAction,
    ApprovalRateLimitError,
    ApprovalService,
    DuplicateApprovalError,
    InvalidApprovalError,
    MemoryApprovalStore,
)
from server.alerts.identity import ActorGrant
from server.alerts.webhooks import twilio_callback


class FakeEffects:
    def __init__(self) -> None:
        self.executions: list[tuple[str, str, str | None]] = []

    async def execute_paper(
        self, target_id: str, actor_id: str, approval_id: str | None = None
    ) -> dict:
        self.executions.append((target_id, actor_id, approval_id))
        return {"status": "completed", "fills": 2}

    async def execute_live(
        self, target_id: str, actor_id: str, approval_id: str
    ) -> dict:
        self.executions.append((f"live:{target_id}", actor_id, approval_id))
        return {"status": "completed"}

    async def approve_guardian(
        self, target_id: str, actor_id: str, approval_id: str
    ) -> dict:
        self.executions.append((f"guardian:{target_id}", actor_id, approval_id))
        return {"status": "completed"}


@pytest.mark.asyncio
async def test_paper_approval_completes_through_normal_effect() -> None:
    store = MemoryApprovalStore()
    effects = FakeEffects()
    service = ApprovalService(store, effects)
    record, reference = await service.request(
        "actor-1", ApprovalAction.EXECUTE_PAPER, "opp-1", "sms"
    )
    result = await service.respond(reference, "actor-1", "sms", "approve")
    assert result["status"] == "completed"
    assert effects.executions == [("opp-1", "actor-1", record.id)]
    assert store.attempts[-1].outcome == "approved"


@pytest.mark.asyncio
async def test_reject_is_terminal_and_does_not_execute() -> None:
    store = MemoryApprovalStore()
    effects = FakeEffects()
    service = ApprovalService(store, effects)
    _, reference = await service.request(
        "actor-1", ApprovalAction.EXECUTE_PAPER, "opp-1", "sms"
    )
    assert (await service.respond(reference, "actor-1", "sms", "reject"))[
        "status"
    ] == "rejected"
    with pytest.raises(InvalidApprovalError, match="already consumed"):
        await service.respond(reference, "actor-1", "sms", "approve")
    assert effects.executions == []


@pytest.mark.asyncio
async def test_reference_is_actor_and_channel_bound() -> None:
    store = MemoryApprovalStore()
    service = ApprovalService(store, FakeEffects())
    _, reference = await service.request(
        "actor-1", ApprovalAction.EXECUTE_PAPER, "opp-1", "sms"
    )
    with pytest.raises(InvalidApprovalError, match="binding mismatch"):
        await service.respond(reference, "actor-2", "sms", "approve")
    with pytest.raises(InvalidApprovalError, match="binding mismatch"):
        await service.respond(reference, "actor-1", "email", "approve")


@pytest.mark.asyncio
async def test_expired_reference_fails() -> None:
    clock = [datetime(2026, 7, 17, tzinfo=UTC)]
    store = MemoryApprovalStore(lambda: clock[0])
    service = ApprovalService(store, FakeEffects())
    _, reference = await service.request(
        "actor-1",
        ApprovalAction.EXECUTE_PAPER,
        "opp-1",
        "sms",
        ttl=timedelta(seconds=1),
    )
    clock[0] += timedelta(seconds=2)
    with pytest.raises(InvalidApprovalError, match="expired"):
        await service.respond(reference, "actor-1", "sms", "approve")
    replacement, _ = await service.request(
        "actor-1", ApprovalAction.EXECUTE_PAPER, "opp-1", "sms"
    )
    assert replacement.status == "pending"


@pytest.mark.asyncio
@pytest.mark.parametrize("action", [ApprovalAction.LIVE_ORDER, ApprovalAction.GUARDIAN])
async def test_bare_channel_response_never_completes_high_stakes_action(
    action: ApprovalAction,
) -> None:
    store = MemoryApprovalStore()
    effects = FakeEffects()
    service = ApprovalService(store, effects)
    _, reference = await service.request("actor-1", action, "target-1", "sms")
    result = await service.respond(reference, "actor-1", "sms", "approve")
    assert result["status"] == "step_up_required"
    assert effects.executions == []


@pytest.mark.asyncio
async def test_channel_may_safely_reject_high_stakes_action_without_step_up() -> None:
    store = MemoryApprovalStore()
    effects = FakeEffects()
    service = ApprovalService(store, effects)
    _, reference = await service.request(
        "actor-1", ApprovalAction.GUARDIAN, "proposal-1", "sms"
    )
    result = await service.respond(reference, "actor-1", "sms", "reject")
    assert result["status"] == "rejected"
    assert effects.executions == []


def test_approval_service_has_no_caller_supplied_step_up_boolean() -> None:
    assert (
        "step_up_satisfied" not in inspect.signature(ApprovalService.respond).parameters
    )


@pytest.mark.asyncio
async def test_pending_dedup_and_hourly_rate_limit() -> None:
    store = MemoryApprovalStore()
    service = ApprovalService(store, FakeEffects())
    await service.request("actor-1", ApprovalAction.EXECUTE_PAPER, "opp-1", "sms")
    with pytest.raises(DuplicateApprovalError):
        await service.request("actor-1", ApprovalAction.EXECUTE_PAPER, "opp-1", "sms")
    await service.request("actor-1", ApprovalAction.EXECUTE_PAPER, "opp-2", "sms")
    await service.request("actor-1", ApprovalAction.EXECUTE_PAPER, "opp-3", "sms")
    with pytest.raises(ApprovalRateLimitError):
        await service.request("actor-1", ApprovalAction.EXECUTE_PAPER, "opp-4", "sms")


@pytest.mark.asyncio
async def test_every_response_attempt_is_audited() -> None:
    store = MemoryApprovalStore()
    service = ApprovalService(store, FakeEffects())
    _, reference = await service.request(
        "actor-1", ApprovalAction.EXECUTE_PAPER, "opp-1", "sms"
    )
    with pytest.raises(InvalidApprovalError):
        await service.respond("unknown", "actor-1", "sms", "approve")
    await service.respond(reference, "actor-1", "sms", "reject")
    assert [(item.outcome, item.reason) for item in store.attempts] == [
        ("denied", "unknown reference"),
        ("rejected", "accepted"),
    ]


@pytest.mark.asyncio
async def test_signed_twilio_response_round_trip(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    store = MemoryApprovalStore()
    effects = FakeEffects()
    service = ApprovalService(store, effects)
    record, reference = await service.request(
        "actor-1", ApprovalAction.EXECUTE_PAPER, "opp-1", "sms"
    )
    public_url = "https://alerts.example/callbacks/twilio"
    params = {"From": "+15550000002", "Body": f"APPROVE {reference}"}
    signature = expected_signature(public_url, params, "test-token")
    encoded = (f"From=%2B15550000002&Body=APPROVE+{reference}").encode()
    delivered = False

    async def receive() -> dict:
        nonlocal delivered
        if delivered:
            return {"type": "http.request", "body": b"", "more_body": False}
        delivered = True
        return {"type": "http.request", "body": encoded, "more_body": False}

    request = Request(
        {
            "type": "http",
            "method": "POST",
            "path": "/callbacks/twilio",
            "headers": [(b"x-twilio-signature", signature.encode())],
        },
        receive,
    )

    async def resolve(channel: str, user_id: str) -> ActorGrant:
        assert (channel, user_id) == ("sms", "+15550000002")
        return ActorGrant("actor-1", 3, {})

    monkeypatch.setenv("AETHER_COMMS__TWILIO_TOKEN", "test-token")
    monkeypatch.setenv("AETHER_COMMS__TWILIO_WEBHOOK_URL", public_url)
    monkeypatch.setattr("server.alerts.webhooks.resolve_channel_actor", resolve)
    result = await twilio_callback(request, service)
    assert result["status"] == "completed"
    assert effects.executions == [("opp-1", "actor-1", record.id)]
