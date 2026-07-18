"""Single-use, expiring, action-bound approval orchestration."""

from __future__ import annotations

import asyncio
import hashlib
import secrets
from collections.abc import Callable
from dataclasses import dataclass, replace
from datetime import UTC, datetime, timedelta
from enum import StrEnum
from typing import Any, Protocol

import asyncpg
import ulid

from server.alerts.history import get_pool


class ApprovalAction(StrEnum):
    EXECUTE_PAPER = "execute_paper"
    LIVE_ORDER = "live_order"
    GUARDIAN = "guardian"


class ApprovalError(RuntimeError):
    """Base approval failure."""


class DuplicateApprovalError(ApprovalError):
    pass


class ApprovalRateLimitError(ApprovalError):
    pass


class InvalidApprovalError(ApprovalError):
    pass


@dataclass(frozen=True)
class ApprovalRecord:
    id: str
    token_hash: str
    actor_id: str
    action: ApprovalAction
    target_id: str
    channel: str
    requires_step_up: bool
    status: str
    expires_at: datetime
    created_at: datetime
    consumed_at: datetime | None = None


@dataclass(frozen=True)
class ApprovalAttempt:
    approval_id: str | None
    actor_id: str
    channel: str
    decision: str
    outcome: str
    reason: str


class ApprovalStore(Protocol):
    async def create(
        self,
        actor_id: str,
        action: ApprovalAction,
        target_id: str,
        channel: str,
        requires_step_up: bool,
        ttl: timedelta,
    ) -> tuple[ApprovalRecord, str]: ...

    async def consume(
        self,
        reference: str,
        actor_id: str,
        channel: str,
        decision: str,
    ) -> ApprovalRecord: ...

    async def mark_failed(self, approval_id: str) -> None: ...


def _token_hash(reference: str) -> str:
    return hashlib.sha256(reference.encode()).hexdigest()


class MemoryApprovalStore:
    """Deterministic store for unit tests and local non-production use."""

    def __init__(self, now: Callable[[], datetime] | None = None) -> None:
        self._now = now or (lambda: datetime.now(UTC))
        self._records: dict[str, ApprovalRecord] = {}
        self._lock = asyncio.Lock()
        self.attempts: list[ApprovalAttempt] = []

    async def create(
        self,
        actor_id: str,
        action: ApprovalAction,
        target_id: str,
        channel: str,
        requires_step_up: bool,
        ttl: timedelta,
    ) -> tuple[ApprovalRecord, str]:
        now = self._now()
        async with self._lock:
            recent = [
                record
                for record in self._records.values()
                if record.actor_id == actor_id
                and record.created_at > now - timedelta(hours=1)
            ]
            if len(recent) >= 3:
                raise ApprovalRateLimitError(
                    "approval notification rate limit exceeded"
                )
            if any(
                record.actor_id == actor_id
                and record.action == action
                and record.target_id == target_id
                and record.status == "pending"
                and record.expires_at > now
                for record in self._records.values()
            ):
                raise DuplicateApprovalError(
                    "an approval is already pending for this action"
                )
            reference = secrets.token_urlsafe(32)
            record = ApprovalRecord(
                id=str(ulid.new()),
                token_hash=_token_hash(reference),
                actor_id=actor_id,
                action=action,
                target_id=target_id,
                channel=channel,
                requires_step_up=requires_step_up,
                status="pending",
                expires_at=now + ttl,
                created_at=now,
            )
            self._records[record.token_hash] = record
            return record, reference

    async def consume(
        self,
        reference: str,
        actor_id: str,
        channel: str,
        decision: str,
    ) -> ApprovalRecord:
        now = self._now()
        async with self._lock:
            record = self._records.get(_token_hash(reference))
            if record is None:
                self.attempts.append(
                    ApprovalAttempt(
                        None, actor_id, channel, decision, "denied", "unknown reference"
                    )
                )
                raise InvalidApprovalError("approval reference is invalid")
            reason = ""
            channel_matches = record.channel == channel
            if record.actor_id != actor_id or not channel_matches:
                reason = "actor or channel binding mismatch"
            elif record.status != "pending":
                reason = "approval reference was already consumed"
            elif record.expires_at <= now:
                reason = "approval reference expired"
                self._records[record.token_hash] = replace(record, status="expired")
            elif decision not in {"approve", "reject"}:
                reason = "invalid approval decision"
            elif decision == "approve" and record.requires_step_up:
                reason = "fresh step-up must complete through the authenticated action endpoint"
            if reason:
                self.attempts.append(
                    ApprovalAttempt(
                        record.id, actor_id, channel, decision, "denied", reason
                    )
                )
                raise InvalidApprovalError(reason)
            status = "approved" if decision == "approve" else "rejected"
            consumed = replace(record, status=status, consumed_at=now)
            self._records[record.token_hash] = consumed
            self.attempts.append(
                ApprovalAttempt(
                    record.id, actor_id, channel, decision, status, "accepted"
                )
            )
            return consumed

    async def mark_failed(self, approval_id: str) -> None:
        async with self._lock:
            for token_hash, record in self._records.items():
                if record.id == approval_id:
                    self._records[token_hash] = replace(record, status="failed")
                    return


class PostgresApprovalStore:
    """Production store with row-lock consumption and append-only attempts."""

    async def create(
        self,
        actor_id: str,
        action: ApprovalAction,
        target_id: str,
        channel: str,
        requires_step_up: bool,
        ttl: timedelta,
    ) -> tuple[ApprovalRecord, str]:
        pool = await get_pool()
        reference = secrets.token_urlsafe(32)
        token_hash = _token_hash(reference)
        approval_id = str(ulid.new())
        async with pool.acquire() as conn, conn.transaction():
            await conn.execute(
                """UPDATE approval_references
                   SET status='expired', updated_ts=now()
                   WHERE actor_id=$1 AND action=$2 AND target_id=$3
                     AND status='pending' AND expires_ts <= now()""",
                actor_id,
                action.value,
                target_id,
            )
            recent = await conn.fetchval(
                """SELECT count(*) FROM approval_references
                   WHERE actor_id = $1 AND created_ts > now() - interval '1 hour'""",
                actor_id,
            )
            if recent >= 3:
                raise ApprovalRateLimitError(
                    "approval notification rate limit exceeded"
                )
            try:
                row = await conn.fetchrow(
                    """INSERT INTO approval_references
                       (id, token_hash, actor_id, action, target_id, channel,
                        requires_step_up, expires_ts)
                       VALUES ($1,$2,$3,$4,$5,$6,$7,now()+$8::interval)
                       RETURNING *""",
                    approval_id,
                    token_hash,
                    actor_id,
                    action.value,
                    target_id,
                    channel,
                    requires_step_up,
                    f"{int(ttl.total_seconds())} seconds",
                )
                if action == ApprovalAction.GUARDIAN:
                    await conn.execute(
                        """INSERT INTO step_up_challenges
                           (id, token_hash, actor_id, action, target_id,
                            approval_reference_id, expires_ts)
                           VALUES ($1,$2,$3,'guardian_approval',$4,$5,
                                   now()+$6::interval)""",
                        str(ulid.new()),
                        token_hash,
                        actor_id,
                        target_id,
                        approval_id,
                        f"{int(ttl.total_seconds())} seconds",
                    )
            except asyncpg.UniqueViolationError as exc:
                raise DuplicateApprovalError(
                    "an approval is already pending for this action"
                ) from exc
        return _record_from_row(row), reference

    async def consume(
        self,
        reference: str,
        actor_id: str,
        channel: str,
        decision: str,
    ) -> ApprovalRecord:
        pool = await get_pool()
        async with pool.acquire() as conn, conn.transaction():
            row = await conn.fetchrow(
                "SELECT * FROM approval_references WHERE token_hash=$1 FOR UPDATE",
                _token_hash(reference),
            )
            record = _record_from_row(row) if row is not None else None
            reason = ""
            if record is None:
                reason = "unknown reference"
            elif record.actor_id != actor_id or record.channel != channel:
                reason = "actor or channel binding mismatch"
            elif record.status != "pending":
                reason = "approval reference was already consumed"
            elif record.expires_at <= datetime.now(UTC):
                reason = "approval reference expired"
                await conn.execute(
                    "UPDATE approval_references SET status='expired', updated_ts=now() WHERE id=$1",
                    record.id,
                )
            elif decision not in {"approve", "reject"}:
                reason = "invalid approval decision"
            elif decision == "approve" and record.requires_step_up:
                reason = "fresh step-up must complete through the authenticated action endpoint"
            if reason:
                await _insert_attempt(
                    conn,
                    record.id if record else None,
                    actor_id,
                    channel,
                    decision,
                    "denied",
                    reason,
                )
                raise InvalidApprovalError(reason)
            assert record is not None
            status = "approved" if decision == "approve" else "rejected"
            row = await conn.fetchrow(
                """UPDATE approval_references
                   SET status=$2, consumed_ts=now(), updated_ts=now()
                   WHERE id=$1 RETURNING *""",
                record.id,
                status,
            )
            await _insert_attempt(
                conn, record.id, actor_id, channel, decision, status, "accepted"
            )
        return _record_from_row(row)

    async def mark_failed(self, approval_id: str) -> None:
        pool = await get_pool()
        async with pool.acquire() as conn:
            await conn.execute(
                """UPDATE approval_references SET status='failed', updated_ts=now()
                   WHERE id=$1 AND status IN ('pending','approved')""",
                approval_id,
            )


def _record_from_row(row: Any) -> ApprovalRecord:
    return ApprovalRecord(
        id=row["id"],
        token_hash=row["token_hash"],
        actor_id=row["actor_id"],
        action=ApprovalAction(row["action"]),
        target_id=row["target_id"],
        channel=row["channel"],
        requires_step_up=row["requires_step_up"],
        status=row["status"],
        expires_at=row["expires_ts"],
        created_at=row["created_ts"],
        consumed_at=row["consumed_ts"],
    )


async def _insert_attempt(
    conn: Any,
    approval_id: str | None,
    actor_id: str,
    channel: str,
    decision: str,
    outcome: str,
    reason: str,
) -> None:
    await conn.execute(
        """INSERT INTO approval_attempts
           (id, approval_id, actor_id, channel, decision, outcome, reason)
           VALUES ($1,$2,$3,$4,$5,$6,$7)""",
        str(ulid.new()),
        approval_id,
        actor_id,
        channel,
        decision,
        outcome,
        reason,
    )


class ApprovalEffects(Protocol):
    async def execute_paper(
        self, target_id: str, actor_id: str, approval_id: str | None = None
    ) -> dict[str, Any]: ...

    async def execute_live(
        self, target_id: str, actor_id: str, approval_id: str
    ) -> dict[str, Any]: ...

    async def approve_guardian(
        self, target_id: str, actor_id: str, approval_id: str
    ) -> dict[str, Any]: ...


class ApprovalService:
    def __init__(self, store: ApprovalStore, effects: ApprovalEffects) -> None:
        self._store = store
        self._effects = effects

    async def fail(self, approval_id: str) -> None:
        await self._store.mark_failed(approval_id)

    async def request(
        self,
        actor_id: str,
        action: ApprovalAction,
        target_id: str,
        channel: str,
        *,
        ttl: timedelta = timedelta(minutes=10),
    ) -> tuple[ApprovalRecord, str]:
        requires_step_up = action in {
            ApprovalAction.LIVE_ORDER,
            ApprovalAction.GUARDIAN,
        }
        if action == ApprovalAction.GUARDIAN:
            ttl = min(ttl, timedelta(minutes=5))
        return await self._store.create(
            actor_id, action, target_id, channel, requires_step_up, ttl
        )

    async def respond(
        self,
        reference: str,
        actor_id: str,
        channel: str,
        decision: str,
    ) -> dict[str, Any]:
        try:
            record = await self._store.consume(
                reference,
                actor_id,
                channel,
                decision,
            )
        except InvalidApprovalError as exc:
            if "step-up" in str(exc):
                return {"status": "step_up_required", "reason": str(exc)}
            raise
        if record.status == "rejected":
            return {"status": "rejected", "approval_id": record.id}
        try:
            if record.action == ApprovalAction.EXECUTE_PAPER:
                effect = await self._effects.execute_paper(
                    record.target_id, record.actor_id, record.id
                )
            elif record.action == ApprovalAction.LIVE_ORDER:
                effect = await self._effects.execute_live(
                    record.target_id, record.actor_id, record.id
                )
            else:
                effect = await self._effects.approve_guardian(
                    record.target_id, record.actor_id, record.id
                )
        except Exception:
            await self._store.mark_failed(record.id)
            raise
        return {"status": "completed", "approval_id": record.id, "effect": effect}
