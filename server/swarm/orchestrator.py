"""Bounded swarm orchestration with graceful cancellation and one packet."""

from __future__ import annotations

import asyncio
import os
from collections.abc import Awaitable, Callable
from decimal import Decimal
from typing import Any

from pydantic import BaseModel, Field

from server.swarm.budget import (
    BudgetAccountingError,
    BudgetExceededError,
    BudgetLedger,
    BudgetLimits,
)
from server.swarm.models import Finding, WorkerGrant
from server.swarm.packet import DecisionPacket, build_packet
from server.swarm.scratchpad import Scratchpad
from server.swarm.worker import (
    BrainRecallRetriever,
    Completion,
    EvidenceRetriever,
    ResearchWorker,
)


class SwarmRequest(BaseModel):
    question: str = Field(min_length=1, max_length=8_000)
    budget: BudgetLimits
    context: dict[str, Any] = Field(default_factory=dict)
    workers: int = Field(default=3, ge=1, le=8)


class ProgressEvent(BaseModel):
    kind: str
    worker_id: str | None = None
    detail: str | None = None


ProgressCallback = Callable[[ProgressEvent], Awaitable[None]]


async def _noop_progress(event: ProgressEvent) -> None:
    del event


class SwarmNoEvidenceError(RuntimeError):
    pass


class SwarmOrchestrator:
    def __init__(
        self,
        *,
        retriever: EvidenceRetriever | None = None,
        completion: Completion | None = None,
        max_cost_per_token_usd: Decimal | None = None,
    ) -> None:
        self.retriever = retriever or BrainRecallRetriever()
        self.completion = completion
        raw_ceiling = os.environ.get("AETHER_SWARM__MAX_COST_PER_TOKEN_USD", "0.0001")
        self.max_cost_per_token_usd = (
            max_cost_per_token_usd
            if max_cost_per_token_usd is not None
            else Decimal(raw_ceiling)
        )
        if self.max_cost_per_token_usd <= 0:
            raise ValueError("swarm token-cost ceiling must be positive")

    async def launch(
        self,
        request: SwarmRequest,
        *,
        progress: ProgressCallback = _noop_progress,
    ) -> DecisionPacket:
        ledger = BudgetLedger(request.budget)
        scratchpad = Scratchpad()
        worker_count = request.workers
        token_allowance = max(1, request.budget.max_tokens // request.budget.max_calls)
        cost_allowance = request.budget.max_cost_usd / Decimal(request.budget.max_calls)
        workers = [self._worker(index) for index in range(worker_count)]
        findings: list[Finding] = []

        await progress(ProgressEvent(kind="started"))

        async def run(worker: ResearchWorker) -> None:
            await progress(
                ProgressEvent(kind="worker_started", worker_id=worker.worker_id)
            )
            try:
                result = await worker.research(
                    question=request.question,
                    context=request.context,
                    ledger=ledger,
                    scratchpad=scratchpad,
                    token_allowance=token_allowance,
                    cost_allowance=cost_allowance,
                )
                findings.extend(result)
                await progress(
                    ProgressEvent(kind="worker_completed", worker_id=worker.worker_id)
                )
            except BudgetExceededError as exc:
                await progress(
                    ProgressEvent(
                        kind="budget_truncated",
                        worker_id=worker.worker_id,
                        detail=exc.dimension,
                    )
                )
            except BudgetAccountingError:
                await ledger.mark_truncated("provider_overage")
                await progress(
                    ProgressEvent(
                        kind="budget_truncated",
                        worker_id=worker.worker_id,
                        detail="provider_overage",
                    )
                )
            except Exception:
                # One failed research branch must not discard cited findings
                # produced by the other branches on this understanding path.
                await progress(
                    ProgressEvent(
                        kind="worker_failed",
                        worker_id=worker.worker_id,
                        detail="worker_error",
                    )
                )

        try:
            async with asyncio.timeout(request.budget.max_seconds):
                # Two waves retain bounded parallelism while guaranteeing that
                # later workers can observe the first wave's shared scratchpad.
                split = max(1, (worker_count + 1) // 2)
                await asyncio.gather(*(run(worker) for worker in workers[:split]))
                await asyncio.gather(*(run(worker) for worker in workers[split:]))
        except TimeoutError:
            await ledger.mark_truncated("seconds")
            await progress(ProgressEvent(kind="budget_truncated", detail="seconds"))

        await ledger.assert_within_limits()
        usage = await ledger.usage()
        if not findings:
            raise SwarmNoEvidenceError("swarm produced no cited Brain findings")
        packet = build_packet(
            question=request.question,
            findings=tuple(findings),
            usage=usage,
            truncated_dimension=ledger.truncated_dimension,
        )
        await progress(ProgressEvent(kind="packet_ready"))
        return packet

    def _worker(self, index: int) -> ResearchWorker:
        worker_id = f"swarm-worker-{index + 1}"
        kwargs: dict[str, Any] = {}
        if self.completion is not None:
            kwargs["completion"] = self.completion
        return ResearchWorker(
            worker_id=worker_id,
            grant=WorkerGrant(actor_id=worker_id),
            retriever=self.retriever,
            max_cost_per_token_usd=self.max_cost_per_token_usd,
            **kwargs,
        )
