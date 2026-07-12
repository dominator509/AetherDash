"""MCP authentication — validates Bearer token against Postgres sessions table.
Full implementation: queries sessions and grants tables for tier-appropriate access.
Falls back to test tokens in dev mode (AETHER_ENV=dev)."""

import hashlib
import os
from dataclasses import dataclass
from datetime import UTC, datetime
from typing import Any

import asyncpg  # type: ignore[import-untyped]

# Test token mapping: Bearer test-{role}
# Available in dev mode only.
_TEST_TOKENS = {
    "test-viewer": 1,
    "test-trader": 3,
    "test-admin": 5,
}

# Deterministic ULIDs for test-token actor identities.
# Each ULID is valid per the canonical ULID spec (26-char Crockford base32).
# Used only in dev mode test-token path; real sessions use DB-generated ULIDs.
_TEST_ULIDS: dict[str, str] = {
    "test-viewer": "01ARZ3NDEKTSV4RRFFQ69G0001",
    "test-trader": "01ARZ3NDEKTSV4RRFFQ69G0002",
    "test-admin": "01ARZ3NDEKTSV4RRFFQ69G0003",
}

# Connection pool — initialized at application startup via init_pool()
_pool: asyncpg.Pool | None = None


@dataclass
class Session:
    session_id: str
    user_id: str
    actor_id: str
    tier: int
    origin_kind: str
    device_label: str | None = None
    scopes: dict[str, Any] | None = None
    grant_tier: int | None = None


class AuthError(Exception):
    pass


class PermissionDeniedError(AuthError):
    """Raised when a session has no valid grant or the grant expired."""

    pass


def _database_url() -> str:
    """Return the Postgres DSN, falling back to the dev default."""
    return os.environ.get(
        "DATABASE_URL",
        "postgres://aether:aether@localhost:5432/aether",
    )


def _is_dev() -> bool:
    return os.environ.get("AETHER_ENV", "prod") == "dev"


async def init_pool() -> None:
    """Create the asyncpg connection pool at application startup.
    In dev mode when DB is unavailable, the pool stays None so test tokens
    can still be used."""
    global _pool
    try:
        _pool = await asyncpg.create_pool(
            _database_url(),
            min_size=2,
            max_size=10,
        )
    except Exception:
        if _is_dev():
            _pool = None  # dev mode: test tokens will still work without DB
        else:
            raise


async def close_pool() -> None:
    """Close the asyncpg connection pool at application shutdown."""
    global _pool
    if _pool is not None:
        await _pool.close()
        _pool = None


async def authenticate(authorization: str | None) -> Session:
    """Validate a Bearer token and return the session.

    Validation order:
    1. Test tokens (test-* prefix) — dev mode only.
    2. Postgres sessions table — hash token, query by token_hash,
       then evaluate permission_grants for tier and scope enforcement.
    3. Fail with AuthError or PermissionDeniedError.
    """
    if not authorization:
        raise AuthError("missing Authorization header")
    token = authorization.removeprefix("Bearer ").strip()

    # 1. Test tokens in dev mode
    if _is_dev():
        tier = _TEST_TOKENS.get(token)
        if tier is not None:
            return Session(
                session_id="test-session",
                user_id=token,
                actor_id=_TEST_ULIDS.get(token, token),
                tier=tier,
                origin_kind="human",
                scopes={},  # empty dict = no scope restriction
                grant_tier=tier,
            )

    # 2. Database lookup — hash token, query by token_hash
    # TODO(EP-401): upgrade to argon2id
    token_hash = hashlib.sha256(token.encode()).hexdigest()

    pool = _pool
    if pool is None:
        raise AuthError("authentication service unavailable")

    try:
        async with pool.acquire() as conn:
            row = await conn.fetchrow(
                "SELECT s.id, s.user_id, s.tier, s.origin_kind, s.device_label "
                "FROM sessions s "
                "WHERE s.token_hash = $1 AND s.expires_ts > now()",
                token_hash,
            )
            if row is None:
                raise AuthError("invalid token")

            # Query permission_grants for the actor.
            # Conflict resolution: ORDER BY tier DESC gets the highest-privilege
            # (highest numeric tier) active grant first; among equal tiers the
            # later-expiring grant wins. Expired grants are filtered in SQL so
            # fetchrow() returns the single best grant deterministically.
            grant_row = await conn.fetchrow(
                "SELECT tier, scopes, expires_ts "
                "FROM permission_grants "
                "WHERE actor_id = $1 AND actor_kind = $2 "
                "  AND (expires_ts IS NULL OR expires_ts > now()) "
                "ORDER BY tier DESC, expires_ts DESC NULLS FIRST, id ASC",
                row["user_id"],
                row["origin_kind"],
            )
            if grant_row is None:
                raise PermissionDeniedError("no grant for this actor")

            # Check grant expiration
            expires_ts = grant_row["expires_ts"]
            if expires_ts is not None and expires_ts < datetime.now(UTC):
                raise PermissionDeniedError("grant expired")

            # Decode scopes: asyncpg returns JSONB as Python objects.
            # Arrays (["tool1", "tool2"]) become lists; normalize to dict.
            grant_scopes: dict[str, Any] = grant_row["scopes"]
            if isinstance(grant_scopes, list):
                grant_scopes = {"allowed": grant_scopes}

            grant_tier = grant_row["tier"]
            effective_tier = min(row["tier"], grant_tier)

            return Session(
                session_id=row["id"],
                user_id=row["user_id"],
                actor_id=row["user_id"],
                tier=effective_tier,
                origin_kind=row["origin_kind"],
                device_label=row.get("device_label"),
                scopes=grant_scopes,
                grant_tier=grant_tier,
            )
    except Exception as exc:
        if isinstance(exc, AuthError):
            raise
        raise AuthError("authentication service unavailable") from None
