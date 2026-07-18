"""Durable and in-memory scheduler state stores."""

from collections.abc import Iterable
from datetime import UTC, datetime
from typing import Protocol

import asyncpg

from server.ingest.models import (
    DowngradeDecision,
    LadderRung,
    SourceConfig,
    SourceState,
)


class StateStore(Protocol):
    async def register(self, configs: Iterable[SourceConfig]) -> None: ...

    async def get(self, source: str) -> SourceState: ...

    async def mark_success(
        self, source: str, cursor: str | None, next_run_at: float
    ) -> None: ...

    async def mark_failure(
        self, source: str, error_code: str, next_run_at: float
    ) -> None: ...

    async def record_downgrade(self, decision: DowngradeDecision) -> None: ...


class MemoryStateStore:
    def __init__(self) -> None:
        self.states: dict[str, SourceState] = {}
        self.downgrades: list[DowngradeDecision] = []

    async def register(self, configs: Iterable[SourceConfig]) -> None:
        for config in configs:
            self.states.setdefault(
                config.source, SourceState(source=config.source, rung=config.rung)
            )

    async def get(self, source: str) -> SourceState:
        return self.states[source]

    async def mark_success(
        self, source: str, cursor: str | None, next_run_at: float
    ) -> None:
        state = self.states[source]
        state.cursor = cursor
        state.health = "healthy"
        state.consecutive_failures = 0
        state.last_error_code = None
        state.next_run_at = next_run_at

    async def mark_failure(
        self, source: str, error_code: str, next_run_at: float
    ) -> None:
        state = self.states[source]
        state.health = "degraded"
        state.consecutive_failures += 1
        state.last_error_code = error_code
        state.next_run_at = next_run_at

    async def record_downgrade(self, decision: DowngradeDecision) -> None:
        self.downgrades.append(decision)


class PostgresStateStore:
    def __init__(self, pool: asyncpg.Pool) -> None:
        self.pool = pool

    async def register(self, configs: Iterable[SourceConfig]) -> None:
        async with self.pool.acquire() as connection, connection.transaction():
            for config in configs:
                await connection.execute(
                    """
                    INSERT INTO ingest_source_state (source,ladder_rung,health)
                    VALUES ($1,$2,$3)
                    ON CONFLICT (source) DO UPDATE
                    SET ladder_rung=EXCLUDED.ladder_rung,
                        health=CASE WHEN ingest_source_state.health='disabled'
                                    THEN 'disabled' ELSE EXCLUDED.health END,
                        updated_ts=now()
                    """,
                    config.source,
                    int(config.rung),
                    "unknown" if config.enabled else "disabled",
                )

    async def get(self, source: str) -> SourceState:
        row = await self.pool.fetchrow(
            """
            SELECT source,ladder_rung,cursor,health,consecutive_failures,
                   last_error_code,extract(epoch FROM next_run_ts) AS next_run_at
            FROM ingest_source_state WHERE source=$1
            """,
            source,
        )
        if row is None:
            raise KeyError(source)
        return SourceState(
            source=row["source"],
            rung=LadderRung(row["ladder_rung"]),
            cursor=row["cursor"],
            health=row["health"],
            consecutive_failures=row["consecutive_failures"],
            last_error_code=row["last_error_code"],
            next_run_at=float(row["next_run_at"]),
        )

    async def mark_success(
        self, source: str, cursor: str | None, next_run_at: float
    ) -> None:
        await self.pool.execute(
            """
            UPDATE ingest_source_state
            SET cursor=$2,health='healthy',consecutive_failures=0,
                last_error_code=NULL,last_success_ts=now(),
                next_run_ts=$3,updated_ts=now()
            WHERE source=$1
            """,
            source,
            cursor,
            datetime.fromtimestamp(next_run_at, UTC),
        )

    async def mark_failure(
        self, source: str, error_code: str, next_run_at: float
    ) -> None:
        await self.pool.execute(
            """
            UPDATE ingest_source_state
            SET health='degraded',consecutive_failures=consecutive_failures+1,
                last_error_code=$2,next_run_ts=$3,updated_ts=now()
            WHERE source=$1
            """,
            source,
            error_code,
            datetime.fromtimestamp(next_run_at, UTC),
        )

    async def record_downgrade(self, decision: DowngradeDecision) -> None:
        import ulid  # noqa: PLC0415

        await self.pool.execute(
            """
            INSERT INTO ingest_rung_decisions
                (id,source,from_rung,to_rung,reason,approved_by)
            VALUES ($1,$2,$3,$4,$5,$6)
            """,
            str(ulid.new()),
            decision.source,
            int(decision.from_rung),
            int(decision.to_rung),
            decision.reason,
            decision.approved_by,
        )
