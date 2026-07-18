"""Atomic pre-authorization for every swarm budget dimension."""

import asyncio
import time
from collections.abc import Callable
from dataclasses import dataclass
from decimal import Decimal
from uuid import uuid4

from pydantic import BaseModel, Field


class BudgetLimits(BaseModel):
    max_calls: int = Field(ge=1, le=64)
    max_tokens: int = Field(ge=1, le=2_000_000)
    max_cost_usd: Decimal = Field(ge=Decimal("0"), le=Decimal("10000"))
    max_seconds: float = Field(gt=0, le=3_600)


class BudgetUsage(BaseModel):
    calls: int = 0
    tokens: int = 0
    cost_usd: Decimal = Decimal("0")
    elapsed_seconds: float = 0.0


class BudgetExceededError(RuntimeError):
    def __init__(self, dimension: str) -> None:
        super().__init__(f"swarm budget exhausted: {dimension}")
        self.dimension = dimension


class BudgetAccountingError(RuntimeError):
    pass


@dataclass(frozen=True)
class Reservation:
    id: str
    tokens: int
    cost_usd: Decimal


class BudgetLedger:
    def __init__(
        self,
        limits: BudgetLimits,
        *,
        clock: Callable[[], float] = time.monotonic,
    ) -> None:
        self.limits = limits
        self.clock = clock
        self.started_at = clock()
        self._calls = 0
        self._tokens = 0
        self._cost = Decimal("0")
        self._reservations: dict[str, Reservation] = {}
        self._lock = asyncio.Lock()
        self.truncated_dimension: str | None = None

    def elapsed(self) -> float:
        return max(0.0, self.clock() - self.started_at)

    def remaining_seconds(self) -> float:
        return max(0.0, self.limits.max_seconds - self.elapsed())

    async def reserve(self, *, tokens: int, cost_usd: Decimal) -> Reservation:
        if tokens < 0 or cost_usd < 0:
            raise ValueError("budget reservations cannot be negative")
        async with self._lock:
            reserved_tokens = sum(item.tokens for item in self._reservations.values())
            reserved_cost = sum(
                (item.cost_usd for item in self._reservations.values()), Decimal("0")
            )
            dimension = None
            if self.elapsed() >= self.limits.max_seconds:
                dimension = "seconds"
            elif self._calls + len(self._reservations) + 1 > self.limits.max_calls:
                dimension = "calls"
            elif self._tokens + reserved_tokens + tokens > self.limits.max_tokens:
                dimension = "tokens"
            elif self._cost + reserved_cost + cost_usd > self.limits.max_cost_usd:
                dimension = "cost_usd"
            if dimension is not None:
                self.truncated_dimension = self.truncated_dimension or dimension
                raise BudgetExceededError(dimension)
            reservation = Reservation(str(uuid4()), tokens, cost_usd)
            self._reservations[reservation.id] = reservation
            return reservation

    async def commit(
        self,
        reservation: Reservation,
        *,
        actual_tokens: int,
        actual_cost_usd: Decimal,
    ) -> None:
        if actual_tokens < 0 or actual_cost_usd < 0:
            raise BudgetAccountingError("actual usage cannot be negative")
        async with self._lock:
            active = self._reservations.pop(reservation.id, None)
            if active is None:
                raise BudgetAccountingError(
                    "reservation is missing or already consumed"
                )
            if actual_tokens > active.tokens or actual_cost_usd > active.cost_usd:
                self.truncated_dimension = (
                    self.truncated_dimension or "provider_overage"
                )
                raise BudgetAccountingError("provider usage exceeded its reservation")
            self._calls += 1
            self._tokens += actual_tokens
            self._cost += actual_cost_usd

    async def cancel(self, reservation: Reservation) -> None:
        async with self._lock:
            self._reservations.pop(reservation.id, None)

    async def mark_truncated(self, dimension: str) -> None:
        async with self._lock:
            self.truncated_dimension = self.truncated_dimension or dimension

    async def usage(self) -> BudgetUsage:
        async with self._lock:
            return BudgetUsage(
                calls=self._calls,
                tokens=self._tokens,
                cost_usd=self._cost,
                elapsed_seconds=self.elapsed(),
            )

    async def assert_within_limits(self) -> None:
        usage = await self.usage()
        if usage.calls > self.limits.max_calls:
            raise AssertionError("call budget overspent")
        if usage.tokens > self.limits.max_tokens:
            raise AssertionError("token budget overspent")
        if usage.cost_usd > self.limits.max_cost_usd:
            raise AssertionError("cost budget overspent")
