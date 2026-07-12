"""ErrorEnvelope model matching SPEC-003 + aether-core error codes."""

import uuid
from enum import StrEnum

from pydantic import BaseModel


class ErrorCode(StrEnum):
    """Closed error code set — SPEC-003 / aether-core ErrorCode."""

    invalid_argument = "invalid_argument"
    unauthenticated = "unauthenticated"
    permission_denied = "permission_denied"
    not_found = "not_found"
    failed_precondition = "failed_precondition"
    unavailable = "unavailable"
    deadline_exceeded = "deadline_exceeded"
    quarantined = "quarantined"
    internal = "internal"

    def is_retryable(self) -> bool:
        """SPEC-006: only unavailable and deadline_exceeded are retryable."""
        return self in (ErrorCode.unavailable, ErrorCode.deadline_exceeded)


class ErrorEnvelope(BaseModel):
    """Standard error envelope for all HTTP error responses.

    Fields match SPEC-003 / aether-core ErrorEnvelope.
    trace_id is auto-generated per instance via the factory.
    """

    code: ErrorCode
    message: str
    retryable: bool
    trace_id: str
    details: str | None = None


def new_error_envelope(
    code: ErrorCode,
    message: str,
    details: str | None = None,
) -> dict[str, object]:
    """Factory: builds an ErrorEnvelope dict with auto-generated trace_id."""
    env = ErrorEnvelope(
        code=code,
        message=message,
        retryable=code.is_retryable(),
        trace_id=uuid.uuid4().hex,
        details=details,
    )
    return env.model_dump()
