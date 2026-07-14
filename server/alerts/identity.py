"""Authoritative channel identity and permission-grant resolution."""

from dataclasses import dataclass

from server.alerts.history import get_pool


@dataclass(frozen=True)
class ActorGrant:
    actor_id: str
    tier: int
    scopes: dict


async def resolve_channel_actor(
    channel: str, channel_user_id: str
) -> ActorGrant | None:
    """Resolve a verified channel identity and its current, unexpired grant."""
    pool = await get_pool()
    async with pool.acquire() as conn:
        row = await conn.fetchrow(
            """
            SELECT i.actor_id, g.tier, g.scopes
            FROM alert_channel_identities i
            JOIN LATERAL (
                SELECT tier, scopes FROM permission_grants
                WHERE actor_id = i.actor_id AND actor_kind = 'human'
                  AND (expires_ts IS NULL OR expires_ts > now())
                ORDER BY tier DESC LIMIT 1
            ) g ON TRUE
            WHERE i.channel = $1 AND i.channel_user_id = $2
            """,
            channel,
            channel_user_id,
        )
    if row is None:
        return None
    return ActorGrant(row["actor_id"], row["tier"], dict(row["scopes"]))
