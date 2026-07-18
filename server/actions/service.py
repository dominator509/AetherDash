"""Database-backed action orchestration with fail-closed snapshots."""

from __future__ import annotations

import hashlib
from collections.abc import Awaitable, Callable
from datetime import UTC, datetime
from decimal import Decimal
from typing import Any

import asyncpg  # type: ignore[import-untyped]
import ulid

from server.actions.executor import execute_paper
from server.mcp.simulator import run_simulation

_CROCKFORD = "0123456789ABCDEFGHJKMNPQRSTVWXYZ"


class ActionRejectedError(RuntimeError):
    """An action cannot safely complete from current authoritative state."""


def _stable_ulid(namespace: str) -> str:
    """Map a durable database identity to a valid deterministic ULID."""
    value = int.from_bytes(hashlib.sha256(namespace.encode()).digest()[:16], "big")
    chars = []
    for _ in range(26):
        chars.append(_CROCKFORD[value & 31])
        value >>= 5
    return "".join(reversed(chars))


def _iso(value: datetime) -> str:
    return (
        value.astimezone(UTC).isoformat(timespec="milliseconds").replace("+00:00", "Z")
    )


def _decimal(value: Any, field: str) -> str:
    try:
        parsed = Decimal(str(value))
    except Exception as exc:
        raise ActionRejectedError(f"{field} is not a decimal") from exc
    if not parsed.is_finite():
        raise ActionRejectedError(f"{field} must be finite")
    return str(parsed)


def _scope_set(raw: Any) -> tuple[list[str], bool]:
    if raw in ({}, None):
        return [], False
    allowed = (
        raw
        if isinstance(raw, list)
        else raw.get("allowed")
        if isinstance(raw, dict)
        else None
    )
    if not isinstance(allowed, list) or any(
        not isinstance(item, str) for item in allowed
    ):
        raise ActionRejectedError("grant scopes are malformed")
    return allowed, True


class ActionService:
    def __init__(
        self,
        pool: asyncpg.Pool,
        paper_executor: Callable[
            [dict[str, Any]], Awaitable[dict[str, Any]]
        ] = execute_paper,
    ) -> None:
        self._pool = pool
        self._paper_executor = paper_executor

    async def simulate(self, opportunity_id: str, actor_id: str) -> dict[str, Any]:
        async with self._pool.acquire() as conn:
            grant = await self._grant(conn, actor_id, minimum_tier=2, scope="sim.run")
            opportunity = await self._opportunity(conn, opportunity_id)
            payload = await self._simulation_payload(conn, opportunity)
        result = await run_simulation(payload)
        return {
            "status": "completed",
            "opportunity_id": opportunity_id,
            "grant_id": grant["id"],
            "simulation": result,
        }

    async def ignore(self, opportunity_id: str, actor_id: str) -> dict[str, Any]:
        async with self._pool.acquire() as conn, conn.transaction():
            grant = await self._grant(
                conn, actor_id, minimum_tier=1, scope="opportunities.ignore"
            )
            opportunity = await conn.fetchrow(
                "SELECT state,edge->>'net_edge' AS net_edge FROM opportunities WHERE id=$1 FOR UPDATE",
                opportunity_id,
            )
            if opportunity is None:
                raise ActionRejectedError("opportunity does not exist")
            state = opportunity["state"]
            if state == "closed":
                outcome = await conn.fetchval(
                    "SELECT outcome FROM attribution WHERE opportunity_id=$1",
                    opportunity_id,
                )
                if outcome != "ignored":
                    raise ActionRejectedError("opportunity is already terminal")
                return {
                    "status": "completed",
                    "opportunity_id": opportunity_id,
                    "replayed": True,
                }
            if state != "surfaced":
                raise ActionRejectedError("only a surfaced opportunity can be ignored")
            await conn.execute(
                """INSERT INTO opportunity_events
                   (id,opportunity_id,from_state,to_state,actor,detail)
                   VALUES ($1,$2,'surfaced','ignored',$3,$4)""",
                str(ulid.new()),
                opportunity_id,
                actor_id,
                {"reason": "human_ignored"},
            )
            await conn.execute(
                """INSERT INTO attribution
                   (id,opportunity_id,predicted_net_edge,realized_pnl,outcome,closed_ts,reason_ignored,detail)
                   VALUES ($1,$2,$3,0,'ignored',now(),'human_ignored',$4)
                   ON CONFLICT (opportunity_id) DO NOTHING""",
                str(ulid.new()),
                opportunity_id,
                opportunity["net_edge"],
                {"reason": "human_ignored", "realized": "0"},
            )
            await conn.execute(
                """INSERT INTO opportunity_events
                   (id,opportunity_id,from_state,to_state,actor,detail)
                   VALUES ($1,$2,'ignored','closed','attribution',$3)""",
                str(ulid.new()),
                opportunity_id,
                {"reason": "human_ignored"},
            )
        return {
            "status": "completed",
            "opportunity_id": opportunity_id,
            "grant_id": grant["id"],
            "replayed": False,
        }

    async def execute_paper(
        self, opportunity_id: str, actor_id: str, approval_id: str | None
    ) -> dict[str, Any]:
        async with self._pool.acquire() as conn, conn.transaction():
            grant = await self._grant(
                conn, actor_id, minimum_tier=3, scope="orders.submit_paper"
            )
            tier = int(grant["tier"])
            confirmed = tier >= 5
            if approval_id is not None:
                confirmed = await self._approval(
                    conn, approval_id, actor_id, opportunity_id, "execute_paper"
                )
            if tier in {3, 4} and not confirmed:
                raise ActionRejectedError("current tier requires a consumed approval")
            opportunity = await self._opportunity(conn, opportunity_id, for_update=True)
            if opportunity["state"] not in {"surfaced", "accepted", "executed"}:
                raise ActionRejectedError("opportunity is not executable")
            requests = await self._paper_requests(
                conn, opportunity, actor_id, grant, approval_id
            )
            if opportunity["state"] not in {"accepted", "executed"}:
                await conn.execute(
                    """INSERT INTO opportunity_events
                       (id,opportunity_id,from_state,to_state,actor,detail)
                       VALUES ($1,$2,$3,'accepted',$4,$5)""",
                    str(ulid.new()),
                    opportunity_id,
                    opportunity["state"],
                    actor_id,
                    {"approval_id": approval_id},
                )

        results = []
        for request in requests:
            results.append(await self._paper_executor(request))
        async with self._pool.acquire() as conn, conn.transaction():
            state = await conn.fetchval(
                "SELECT state FROM opportunities WHERE id=$1 FOR UPDATE", opportunity_id
            )
            if state == "accepted":
                await conn.execute(
                    """INSERT INTO opportunity_events
                       (id,opportunity_id,from_state,to_state,actor,detail)
                       VALUES ($1,$2,'accepted','executed',$3,$4)""",
                    str(ulid.new()),
                    opportunity_id,
                    actor_id,
                    {
                        "order_ids": [result["order_id"] for result in results],
                        "approval_id": approval_id,
                    },
                )
            elif state != "executed":
                raise ActionRejectedError(
                    "opportunity lifecycle changed during execution"
                )
        return {
            "status": "completed",
            "opportunity_id": opportunity_id,
            "orders": results,
            "replayed": all(bool(result.get("replayed")) for result in results),
        }

    async def _grant(
        self, conn: asyncpg.Connection, actor_id: str, minimum_tier: int, scope: str
    ) -> asyncpg.Record:
        row = await conn.fetchrow(
            """SELECT id, actor_id, actor_kind, tier, scopes, expires_ts, revoked_ts
               FROM permission_grants
               WHERE actor_id=$1 AND actor_kind='human' AND revoked_ts IS NULL
                 AND (expires_ts IS NULL OR expires_ts > now())
               ORDER BY tier DESC, expires_ts DESC NULLS FIRST, id ASC LIMIT 1""",
            actor_id,
        )
        if row is None or int(row["tier"]) < minimum_tier:
            raise ActionRejectedError("current grant does not permit this action")
        scopes, restricted = _scope_set(row["scopes"])
        if restricted and scope not in scopes:
            raise ActionRejectedError("current grant scope does not permit this action")
        return row

    async def _approval(
        self,
        conn: asyncpg.Connection,
        approval_id: str,
        actor_id: str,
        target_id: str,
        action: str,
    ) -> bool:
        row = await conn.fetchrow(
            """SELECT actor_id, target_id, action, status, expires_ts, consumed_ts
               FROM approval_references WHERE id=$1""",
            approval_id,
        )
        if (
            row is None
            or row["actor_id"] != actor_id
            or row["target_id"] != target_id
            or row["action"] != action
            or row["status"] != "approved"
            or row["consumed_ts"] is None
            or row["expires_ts"] <= datetime.now(UTC)
        ):
            raise ActionRejectedError("approval is not current and action-bound")
        return True

    async def _opportunity(
        self, conn: asyncpg.Connection, opportunity_id: str, *, for_update: bool = False
    ) -> asyncpg.Record:
        suffix = " FOR UPDATE" if for_update else ""
        row = await conn.fetchrow(
            "SELECT id, legs, confidence, state FROM opportunities WHERE id=$1"
            + suffix,
            opportunity_id,
        )
        if row is None:
            raise ActionRejectedError("opportunity does not exist")
        return row

    async def _simulation_payload(
        self, conn: asyncpg.Connection, opportunity: asyncpg.Record
    ) -> dict[str, Any]:
        legs = list(opportunity["legs"])
        if len(legs) < 2:
            raise ActionRejectedError(
                "simulation requires at least two opportunity legs"
            )
        priced = []
        for leg in legs:
            row = await conn.fetchrow(
                """SELECT m.venue,m.kind,m.meta,q.bid,q.ask,q.bid_size,q.ask_size,q.ts
                   FROM markets m JOIN quotes_latest q ON q.market_key=m.key WHERE m.key=$1""",
                leg.get("market"),
            )
            if row is None:
                raise ActionRejectedError("current quote is unavailable")
            side = leg.get("side")
            price = row["ask"] if side in {"buy", "buy_no"} else row["bid"]
            if price is None:
                raise ActionRejectedError("executable quote is unavailable")
            priced.append((side, row, price, leg))
        buy = next((item for item in priced if item[0] in {"buy", "buy_no"}), None)
        sell = next((item for item in priced if item[0] in {"sell", "sell_no"}), None)
        if buy is None or sell is None:
            raise ActionRejectedError("simulation requires a buy and sell leg")
        notional = buy[3].get("size_hint") or sell[3].get("size_hint")
        if notional is None:
            raise ActionRejectedError("simulation requires a size hint")
        now = datetime.now(UTC)
        max_age = max(
            0, int((now - min(buy[1]["ts"], sell[1]["ts"])).total_seconds() * 1000)
        )

        def book(item: tuple[Any, Any, Any, Any]) -> dict[str, Any]:
            _, row, _, leg = item
            level_size = row["ask_size"] or row["bid_size"]
            if level_size is None or Decimal(str(level_size)) <= 0:
                raise ActionRejectedError("simulation requires positive visible depth")
            return {
                "market": leg["market"],
                "bids": []
                if row["bid"] is None
                else [
                    {
                        "price": _decimal(row["bid"], "bid"),
                        "size": _decimal(level_size, "depth"),
                    }
                ],
                "asks": []
                if row["ask"] is None
                else [
                    {
                        "price": _decimal(row["ask"], "ask"),
                        "size": _decimal(level_size, "depth"),
                    }
                ],
                "depth": 1,
                "ts": _iso(row["ts"]),
            }

        chain_ids = [dict(item[1]["meta"]).get("chain_id") for item in (buy, sell)]
        probability_kinds = {
            "binary_contract",
            "categorical_contract",
            "scalar_contract",
        }
        return {
            "buy_price": _decimal(buy[2], "buy price"),
            "sell_price": _decimal(sell[2], "sell price"),
            "price_kind": "probability"
            if buy[1]["kind"] in probability_kinds
            and sell[1]["kind"] in probability_kinds
            else "currency",
            "notional": _decimal(notional, "notional"),
            "buy_book": book(buy),
            "sell_book": book(sell),
            "funding_rate": "0",
            "hold_hours": "0",
            "max_quote_age_ms": max_age,
            "tick_stale_ms": 5_000,
            "confidence": _decimal(opportunity["confidence"], "confidence"),
            "is_cross_chain": all(chain_ids) and chain_ids[0] != chain_ids[1],
            "buy_venue": buy[1]["venue"],
            "sell_venue": sell[1]["venue"],
        }

    async def _paper_requests(
        self,
        conn: asyncpg.Connection,
        opportunity: asyncpg.Record,
        actor_id: str,
        grant: asyncpg.Record,
        approval_id: str | None,
    ) -> list[dict[str, Any]]:
        legs = list(opportunity["legs"])
        if not legs:
            raise ActionRejectedError("opportunity has no executable legs")
        caps_row = await conn.fetchrow(
            "SELECT version, body FROM caps WHERE state='active' AND active=true"
        )
        if caps_row is None:
            raise ActionRejectedError("active caps are unavailable")
        caps = dict(caps_row["body"])
        caps["version"] = _stable_ulid(f"caps:{caps_row['version']}")
        if not isinstance(caps.get("per_order_max"), dict) or not isinstance(
            caps.get("daily_max"), dict
        ):
            raise ActionRejectedError(
                "active caps body is not a canonical CapsSnapshot"
            )
        now = datetime.now(UTC)
        daily_notional = await conn.fetchval(
            """SELECT COALESCE(sum(o.price * o.size), 0)
               FROM orders o JOIN order_intents i ON i.id=o.intent_id
               WHERE o.paper=true AND o.created_ts >= date_trunc('day', now())
                 AND i.origin->>'actor_id'=$1""",
            actor_id,
        )
        requests = []
        for index, leg in enumerate(legs):
            market_key = leg.get("market")
            row = await conn.fetchrow(
                """SELECT m.*, v.enabled, q.bid, q.ask, q.mid, q.last, q.bid_size,
                          q.ask_size, q.ts AS quote_ts, q.source, q.seq
                   FROM markets m JOIN venues v ON v.slug=m.venue
                   JOIN quotes_latest q ON q.market_key=m.key WHERE m.key=$1""",
                market_key,
            )
            if row is None:
                raise ActionRejectedError("market or current quote is unavailable")
            side = leg.get("side")
            if side not in {"buy", "sell", "buy_no", "sell_no"}:
                raise ActionRejectedError("opportunity leg side is invalid")
            size = leg.get("size_hint")
            if size is None or Decimal(str(size)) <= 0:
                raise ActionRejectedError(
                    "paper execution requires a positive size hint"
                )
            balance = await conn.fetchrow(
                """SELECT currency, free, locked FROM paper_balances
                   WHERE actor_id=$1 AND venue=$2 ORDER BY currency LIMIT 1""",
                actor_id,
                row["venue"],
            )
            if balance is None:
                raise ActionRejectedError("explicit paper balance is unavailable")
            position = await conn.fetchval(
                "SELECT size FROM positions WHERE market_key=$1 AND paper=true",
                market_key,
            )
            quote = {
                "market": market_key,
                **{
                    name: _decimal(row[name], name)
                    for name in ("bid", "ask", "mid", "last", "bid_size", "ask_size")
                    if row[name] is not None
                },
                "ts": _iso(row["quote_ts"]),
                "source": row["source"],
                **({"seq": row["seq"]} if row["seq"] is not None else {}),
            }
            levels_size = _decimal(
                row["ask_size"] or row["bid_size"] or size, "book size"
            )
            book = {
                "market": market_key,
                "bids": []
                if row["bid"] is None
                else [{"price": _decimal(row["bid"], "bid"), "size": levels_size}],
                "asks": []
                if row["ask"] is None
                else [{"price": _decimal(row["ask"], "ask"), "size": levels_size}],
                "depth": 1,
                "ts": _iso(row["quote_ts"]),
                **({"seq": row["seq"]} if row["seq"] is not None else {}),
            }
            kind = row["kind"]
            size_unit = (
                "contracts"
                if kind
                in {
                    "binary_contract",
                    "categorical_contract",
                    "scalar_contract",
                    "option",
                }
                else "shares"
                if kind == "equity"
                else "base"
            )
            intent_id = _stable_ulid(f"paper:{opportunity['id']}:{index}")
            limit_price = leg.get("target_price")
            request = {
                "actor": {"id": actor_id, "kind": "human"},
                "approval_id": None if int(grant["tier"]) >= 5 else approval_id,
                "intent": {
                    "id": intent_id,
                    "market": market_key,
                    "side": side,
                    "order_type": "limit" if limit_price is not None else "market",
                    **(
                        {"limit_price": _decimal(limit_price, "target price")}
                        if limit_price is not None
                        else {}
                    ),
                    "size": _decimal(size, "size"),
                    "size_unit": size_unit,
                    "tif": "day",
                    "paper": True,
                    "origin": {
                        "kind": "human",
                        "tier": int(grant["tier"]),
                        "actor_id": actor_id,
                    },
                    "quote_snapshot": quote,
                    "caps_version": caps["version"],
                    "created_ts": _iso(now),
                },
                "book": book,
                "risk": {
                    "evaluated_at": _iso(now),
                    "markets": [
                        {
                            "key": row["key"],
                            "venue": row["venue"],
                            "kind": kind,
                            "title": row["title"],
                            "description_ref": row["description_ref"],
                            "status": row["status"],
                            "close_ts": None
                            if row["close_ts"] is None
                            else _iso(row["close_ts"]),
                            "resolve_ts": None
                            if row["resolve_ts"] is None
                            else _iso(row["resolve_ts"]),
                            "outcome": row["outcome"],
                            "jurisdiction_flags": list(row["jurisdiction_flags"]),
                            "venue_ref": dict(row["venue_ref"]),
                            "meta": dict(row["meta"]),
                        }
                    ],
                    "balances": [
                        {
                            "venue": row["venue"],
                            "free": _decimal(balance["free"], "free balance"),
                            "locked": _decimal(balance["locked"], "locked balance"),
                            "currency": balance["currency"],
                        }
                    ],
                    "positions": []
                    if position is None
                    else [
                        {
                            "market": market_key,
                            "outcome": "no" if side == "sell_no" else "yes",
                            "size": _decimal(position, "position"),
                        }
                    ],
                    "venue_health": [
                        {
                            "venue": row["venue"],
                            "status": "ok" if row["enabled"] else "disabled",
                            "breaker_open": not row["enabled"],
                        }
                    ],
                    "active_caps": caps,
                    "caps_by_version": [caps],
                    "daily_notional": _decimal(daily_notional, "daily notional"),
                    "jurisdiction_eligible": [],
                    "live_enabled": False,
                },
                "opportunity_id": opportunity["id"],
            }
            requests.append(request)
        return requests
