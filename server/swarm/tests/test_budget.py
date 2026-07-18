import asyncio
import random
from decimal import Decimal

import pytest

from server.swarm.budget import (
    BudgetAccountingError,
    BudgetExceededError,
    BudgetLedger,
    BudgetLimits,
)


def limits(**overrides: object) -> BudgetLimits:
    values: dict[str, object] = {
        "max_calls": 3,
        "max_tokens": 300,
        "max_cost_usd": Decimal("3"),
        "max_seconds": 10,
    }
    values.update(overrides)
    return BudgetLimits(**values)


@pytest.mark.asyncio
@pytest.mark.parametrize(
    ("overrides", "reservations", "dimension"),
    [
        ({"max_calls": 1}, [(1, "0"), (1, "0")], "calls"),
        ({"max_tokens": 5}, [(4, "0"), (2, "0")], "tokens"),
        ({"max_cost_usd": Decimal("1")}, [(1, "0.6"), (1, "0.5")], "cost_usd"),
    ],
)
async def test_each_discrete_dimension_prevents_overspend(
    overrides: dict[str, object],
    reservations: list[tuple[int, str]],
    dimension: str,
) -> None:
    ledger = BudgetLedger(limits(**overrides))
    first = await ledger.reserve(
        tokens=reservations[0][0], cost_usd=Decimal(reservations[0][1])
    )
    await ledger.commit(
        first,
        actual_tokens=reservations[0][0],
        actual_cost_usd=Decimal(reservations[0][1]),
    )
    with pytest.raises(BudgetExceededError, match=dimension):
        await ledger.reserve(
            tokens=reservations[1][0], cost_usd=Decimal(reservations[1][1])
        )


@pytest.mark.asyncio
async def test_elapsed_time_trips_before_a_call() -> None:
    now = [0.0]
    ledger = BudgetLedger(limits(max_seconds=1), clock=lambda: now[0])
    now[0] = 1.0
    with pytest.raises(BudgetExceededError, match="seconds"):
        await ledger.reserve(tokens=1, cost_usd=Decimal("0"))


@pytest.mark.asyncio
async def test_concurrent_randomized_reservations_never_overspend() -> None:
    rng = random.Random(205)
    ledger = BudgetLedger(
        limits(max_calls=12, max_tokens=200, max_cost_usd=Decimal("2"))
    )

    async def attempt(tokens: int, cost: Decimal) -> None:
        try:
            reservation = await ledger.reserve(tokens=tokens, cost_usd=cost)
        except BudgetExceededError:
            return
        await asyncio.sleep(0)
        await ledger.commit(reservation, actual_tokens=tokens, actual_cost_usd=cost)

    requests = [
        (rng.randint(1, 30), Decimal(rng.randint(1, 30)) / Decimal("100"))
        for _ in range(100)
    ]
    await asyncio.gather(*(attempt(tokens, cost) for tokens, cost in requests))
    await ledger.assert_within_limits()
    usage = await ledger.usage()
    assert usage.calls <= 12
    assert usage.tokens <= 200
    assert usage.cost_usd <= Decimal("2")


@pytest.mark.asyncio
async def test_provider_cannot_report_more_than_pre_authorized() -> None:
    ledger = BudgetLedger(limits())
    reservation = await ledger.reserve(tokens=10, cost_usd=Decimal("0.50"))
    with pytest.raises(BudgetAccountingError, match="exceeded"):
        await ledger.commit(
            reservation, actual_tokens=11, actual_cost_usd=Decimal("0.50")
        )
