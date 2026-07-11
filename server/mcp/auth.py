"""MCP auth stub — validates Bearer token, returns session tier.
Full implementation (EP-401): query sessions table, verify grants."""

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
    tier = _TEST_TOKENS.get(token)
    if tier is None:
        raise AuthError("invalid token")
    return Session(actor_id=token, tier=tier)
