import httpx
import pytest

from connectors.comms.twilio.sender import format_sms, send_alert
from connectors.comms.twilio.webhook import expected_signature, verify_signature
from server.alerts.models import AlertPayload


def payload() -> AlertPayload:
    return AlertPayload(
        alert_id="01ARZ3NDEKTSV4RRFFQ69G5FAV",
        rule_name="edge",
        opportunity_id="01ARZ3NDEKTSV4RRFFQ69G5FAV",
        channel="sms",
        summary="Cross-venue edge",
        net_edge="0.04",
        confidence=0.9,
        action="simulate",
        inline_actions=["simulate", "execute", "ignore"],
    )


def test_signature_round_trip_and_tamper_rejection() -> None:
    url = "https://alerts.example/callbacks/twilio"
    params = {"From": "+15551234567", "Body": "APPROVE token"}
    signature = expected_signature(url, params, "test-token")
    assert verify_signature(url, params, signature, "test-token")
    assert not verify_signature(
        url, {**params, "Body": "APPROVE other"}, signature, "test-token"
    )


@pytest.mark.asyncio
async def test_send_uses_twilio_api(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("AETHER_COMMS__TWILIO_SID", "ACtest")
    monkeypatch.setenv("AETHER_COMMS__TWILIO_TOKEN", "test-token")
    monkeypatch.setenv("AETHER_COMMS__TWILIO_FROM", "+15550000001")
    monkeypatch.setenv("AETHER_COMMS__TWILIO_TO", "+15550000002")

    async def handler(request: httpx.Request) -> httpx.Response:
        assert request.url.path.endswith("/Accounts/ACtest/Messages.json")
        return httpx.Response(201, json={"sid": "SM123"})

    async with httpx.AsyncClient(transport=httpx.MockTransport(handler)) as client:
        assert await send_alert(payload(), client=client) == "SM123"
    assert "Cross-venue edge" in format_sms(payload())
