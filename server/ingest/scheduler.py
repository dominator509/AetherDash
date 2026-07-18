"""Bounded, cursor-safe ingestion scheduler."""

import asyncio
import time
from collections.abc import Awaitable, Callable
from dataclasses import dataclass
from typing import Protocol

import ulid

from server.brain.models import ObjectDraft
from server.ingest.models import (
    DowngradeDecision,
    FetchBatch,
    LadderRung,
    SourceConfig,
)
from server.ingest.state import StateStore


class SourceAdapter(Protocol):
    rung: LadderRung

    async def fetch(self, cursor: str | None, limit: int) -> FetchBatch: ...


StoreDraft = Callable[..., Awaitable[object]]


@dataclass(frozen=True)
class _BatchJob:
    config: SourceConfig
    batch: FetchBatch


async def _default_store_draft(*args: object, **kwargs: object) -> object:
    from server.brain.service import store_draft  # noqa: PLC0415

    return await store_draft(*args, **kwargs)


class FleetScheduler:
    def __init__(
        self,
        state: StateStore,
        *,
        queue_size: int = 32,
        store_draft: StoreDraft = _default_store_draft,
        clock: Callable[[], float] = time.time,
    ) -> None:
        if queue_size < 1:
            raise ValueError("queue_size must be positive")
        self.state = state
        self.queue: asyncio.Queue[_BatchJob] = asyncio.Queue(maxsize=queue_size)
        self.store_draft = store_draft
        self.clock = clock
        self._sources: dict[str, tuple[SourceConfig, SourceAdapter]] = {}
        self._inflight: set[str] = set()
        self._poll_lock = asyncio.Lock()

    async def register(
        self,
        config: SourceConfig,
        adapter: SourceAdapter,
        *,
        downgrade: DowngradeDecision | None = None,
    ) -> None:
        if config.rung != adapter.rung:
            if (
                downgrade is None
                or downgrade.source != config.source
                or downgrade.from_rung != config.rung
                or downgrade.to_rung != adapter.rung
                or adapter.rung <= config.rung
            ):
                raise ValueError(
                    "lower-compliance adapter requires an explicit matching downgrade decision"
                )
            await self.state.record_downgrade(downgrade)
            config = config.model_copy(update={"rung": adapter.rung})
        self._sources[config.source] = (config, adapter)
        await self.state.register([config])

    async def poll_due(self) -> int:
        """Fetch due sources only while downstream queue capacity exists."""
        async with self._poll_lock:
            queued = 0
            for source in sorted(self._sources):
                if self.queue.full():
                    break
                if source in self._inflight:
                    continue
                config, adapter = self._sources[source]
                state = await self.state.get(source)
                if not config.enabled or state.health == "disabled":
                    continue
                now = self.clock()
                if state.next_run_at > now:
                    continue
                try:
                    batch = await adapter.fetch(state.cursor, config.batch_size)
                except Exception as exc:
                    delay = min(
                        config.interval_seconds * (2**state.consecutive_failures),
                        3600,
                    )
                    await self.state.mark_failure(
                        source, type(exc).__name__, now + delay
                    )
                    continue
                self.queue.put_nowait(_BatchJob(config=config, batch=batch))
                self._inflight.add(source)
                queued += 1
            return queued

    async def process_one(self) -> int:
        job = await self.queue.get()
        try:
            for item in job.batch.items:
                if item.source != job.config.source:
                    raise ValueError(
                        "fetched item source does not match registered source"
                    )
                await self.store_draft(
                    ObjectDraft(
                        kind=item.kind, content=item.content, source=item.source
                    ),
                    origin="ingest_fleet",
                    trust=item.trust,
                    raw_content=item.raw_content,
                    ladder_rung=int(job.config.rung),
                    trace_id=str(ulid.new()),
                )
            await self.state.mark_success(
                job.config.source,
                job.batch.next_cursor,
                self.clock() + job.config.interval_seconds,
            )
            return len(job.batch.items)
        except Exception as exc:
            state = await self.state.get(job.config.source)
            delay = min(
                job.config.interval_seconds * (2**state.consecutive_failures), 3600
            )
            await self.state.mark_failure(
                job.config.source, type(exc).__name__, self.clock() + delay
            )
            return 0
        finally:
            self._inflight.discard(job.config.source)
            self.queue.task_done()

    async def worker(self, stop: asyncio.Event) -> None:
        while not stop.is_set():
            try:
                await asyncio.wait_for(self.process_one(), timeout=0.5)
            except TimeoutError:
                continue
