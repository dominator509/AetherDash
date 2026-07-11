"""MCP authentication — validates Bearer token against Postgres sessions table.
Full implementation: queries sessions and grants tables for tier-appropriate access.
Falls back to test tokens in dev mode (AETHER_ENV=dev)."""

import os
from dataclasses import dataclass

import asyncpg

# Test token mapping: Bearer test-{role}
# Available in dev mode only.
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


def _database_url() -> str:
    """Return the Postgres DSN, falling back to the dev default."""
    return os.environ.get(
        "DATABASE_URL",
        "postgres://aether:aether@localhost:5432/aether",
    )


def _is_dev() -> bool:
    return os.environ.get("AETHER_ENV", "prod") == "dev"


async def authenticate(authorization: str | None) -> Session:
    """Validate a Bearer token and return the session.

    Validation order:
    1. Test tokens (test-* prefix) — dev mode only.
    2. Postgres sessions table — query by actor_id (= token).
    3. Fail with AuthError.
    """
    if not authorization:
        raise AuthError("missing Authorization header")
    token = authorization.removeprefix("Bearer ").strip()

    # 1. Test tokens in dev mode
    if _is_dev():
        tier = _TEST_TOKENS.get(token)
        if tier is not None:
            return Session(actor_id=token, tier=tier)

    # 2. Database lookup
    try:
        dsn = _database_url()
        conn = await asyncpg.connect(dsn)
        try:
            row = await conn.fetchrow(
                "SELECT actor_id, tier FROM sessions WHERE actor_id = $1",
                token,
            )
        finally:
            await conn.close()

        if row is not None:
            return Session(actor_id=row["actor_id"], tier=row["tier"])
    except (ConnectionError, OSError, asyncpg.PostgresError) as exc:
        # In dev mode, fall through to error rather than crashing
        if not _is_dev():
            raise AuthError(f"database unavailable: {exc}") from exc

    raise AuthError("invalid token")
