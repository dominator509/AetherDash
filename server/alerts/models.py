"""Alert message model — SPEC-003 AlertMsg envelope."""

from datetime import UTC, datetime

from pydantic import BaseModel, ConfigDict, Field


class AlertPayload(BaseModel):
    """Payload inside AlertMsg."""

    alert_id: str
    rule_name: str
    opportunity_id: str
    channel: str
    summary: str
    net_edge: str
    confidence: float
    action: str
    inline_actions: list[str]
    message_id: str | None = None
    status: str = "pending"
    attempts: int = 0
    last_error: str | None = None


class AlertMsg(BaseModel):
    """Alert message envelope following SPEC-003.

    Serializes ``schema_`` as ``schema`` in JSON.
    """

    model_config = ConfigDict(populate_by_name=True)

    schema_: str = Field(default="aether.alert.v1", alias="schema")
    trace_id: str
    ts: str
    payload: AlertPayload


def now_iso() -> str:
    """Return current UTC time as ISO 8601 with millisecond precision."""
    now = datetime.now(UTC)
    ms = now.microsecond // 1000
    return now.strftime(f"%Y-%m-%dT%H:%M:%S.{ms:03d}Z")
