"""MCP auth stub — validates Bearer token, returns session tier.
Full implementation (EP-401): query sessions table, verify grants."""

import os
from dataclasses import dataclass

# Test token mapping: Bearer test-{role}
# TODO(EP-401): replace with real session table lookup
_TEST_TOKENS = {
    "test-viewer": 1,
    "test-trader": 3,
    "test-admin": 5,
}


@dataclass
class Session:
    actor_id: str
    tier: int


class AuthError(Exception):
    pass


def authenticate(authorization: str | None) -> Session:
    if not authorization:
        raise AuthError("missing Authorization header")
    token = authorization.removeprefix("Bearer ").strip()

    # Test tokens: ONLY available in dev mode (EP-401: replace with DB lookup).
    if os.environ.get("AETHER_ENV", "prod") == "dev":
        tier = _TEST_TOKENS.get(token)
        if tier is not None:
            return Session(actor_id=token, tier=tier)

    raise AuthError("invalid token")
