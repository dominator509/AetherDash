import pytest

from connectors.comms.email.sender import format_email, send_alert
from server.alerts.models import AlertPayload


def make_payload() -> AlertPayload:
    return AlertPayload(
        alert_id="01ARZ3NDEKTSV4RRFFQ69G5FAV",
        rule_name="edge",
        opportunity_id="01ARZ3NDEKTSV4RRFFQ69G5FAV",
        channel="email",
        summary="Cross-venue edge",
        net_edge="0.04",
        confidence=0.9,
        action="simulate",
        inline_actions=["simulate", "execute", "ignore"],
    )


def test_email_format_snapshot() -> None:
    payload = make_payload()
    message = format_email(payload)
    assert message["Subject"] == "AETHER alert: edge"
    assert message.get_content() == (
        "Cross-venue edge\nOpportunity: 01ARZ3NDEKTSV4RRFFQ69G5FAV\n"
        "Net edge: 0.04\nConfidence: 0.90\n"
        "Open AETHER to simulate, execute, or ignore this alert.\n"
    )


@pytest.mark.asyncio
async def test_smtp_stub_round_trip(monkeypatch: pytest.MonkeyPatch) -> None:
    sent: list[object] = []

    class StubSmtp:
        def __init__(self, host: str, port: int, timeout: int) -> None:
            assert (host, port, timeout) == ("smtp.example", 587, 15)

        def __enter__(self) -> "StubSmtp":
            return self

        def __exit__(self, *args: object) -> None:
            return None

        def starttls(self) -> None:
            pass

        def login(self, username: str, password: str) -> None:
            assert (username, password) == ("test-user", "test-password")

        def send_message(self, message: object) -> dict:
            sent.append(message)
            return {}

    monkeypatch.setenv("AETHER_COMMS__SMTP_HOST", "smtp.example")
    monkeypatch.setenv("AETHER_COMMS__SMTP_USERNAME", "test-user")
    monkeypatch.setenv("AETHER_COMMS__SMTP_PASSWORD", "test-password")
    monkeypatch.setenv("AETHER_COMMS__EMAIL_FROM", "from@example.test")
    monkeypatch.setenv("AETHER_COMMS__EMAIL_TO", "to@example.test")
    monkeypatch.setattr("connectors.comms.email.sender.smtplib.SMTP", StubSmtp)
    assert await send_alert(make_payload()) == "smtp-accepted"
    assert len(sent) == 1
