"""Connection-independent, actor-bound, single-use MCP confirmations."""

from __future__ import annotations

import asyncio
import hashlib
import json
import time
from dataclasses import dataclass
from secrets import token_urlsafe
from typing import Any


@dataclass(frozen=True)
class Challenge:
    actor_id: str
    tool_name: str
    payload_hash: str
    expires_at: float


class ConfirmationStore:
    def __init__(self, *, ttl_seconds: float = 300.0) -> None:
        self.ttl_seconds = ttl_seconds
        self._challenges: dict[str, Challenge] = {}
        self._lock = asyncio.Lock()

    @staticmethod
    def payload_hash(payload: dict[str, Any]) -> str:
        encoded = json.dumps(
            payload, sort_keys=True, separators=(",", ":"), ensure_ascii=False
        ).encode("utf-8")
        return hashlib.sha256(encoded).hexdigest()

    async def issue(
        self, *, actor_id: str, tool_name: str, payload: dict[str, Any]
    ) -> str:
        now = time.monotonic()
        ref_id = token_urlsafe(32)
        challenge = Challenge(
            actor_id=actor_id,
            tool_name=tool_name,
            payload_hash=self.payload_hash(payload),
            expires_at=now + self.ttl_seconds,
        )
        async with self._lock:
            self._purge(now)
            self._challenges[ref_id] = challenge
        return ref_id

    async def consume(
        self,
        ref_id: str,
        *,
        actor_id: str,
        tool_name: str,
        payload: dict[str, Any],
    ) -> bool:
        now = time.monotonic()
        async with self._lock:
            self._purge(now)
            challenge = self._challenges.pop(ref_id, None)
        if challenge is None:
            return False
        return (
            challenge.actor_id == actor_id
            and challenge.tool_name == tool_name
            and challenge.payload_hash == self.payload_hash(payload)
            and challenge.expires_at > now
        )

    def _purge(self, now: float) -> None:
        expired = [
            ref_id
            for ref_id, challenge in self._challenges.items()
            if challenge.expires_at <= now
        ]
        for ref_id in expired:
            self._challenges.pop(ref_id, None)
