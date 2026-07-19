import json
from decimal import Decimal
from typing import Any

import pytest

from server.swarm.budget import BudgetExceededError, BudgetLedger, BudgetLimits
from server.swarm.models import BrainCitation, ResearchEvidence, WorkerGrant
from server.swarm.scratchpad import Scratchpad
from server.swarm.worker import ResearchWorker


class FixtureRetriever:
    async def recall(self, query: str, *, k: int) -> tuple[ResearchEvidence, ...]:
        assert query == "Should we enter?"
        assert k == 6
        return (
            ResearchEvidence(
                citation=BrainCitation(object_id="brain-1", provenance_hash="hash-1"),
                text="Demand is rising.",
            ),
        )


@pytest.mark.asyncio
async def test_worker_uses_router_cache_path_and_emits_only_retrieved_citations() -> (
    None
):
    calls: list[dict[str, Any]] = []

    async def completion(
        purpose: str, dynamic_inputs: dict[str, Any], **kwargs: Any
    ) -> dict[str, Any]:
        calls.append({"purpose": purpose, "dynamic_inputs": dynamic_inputs, **kwargs})
        return {
            "text": json.dumps(
                {
                    "findings": [
                        {
                            "claim": "Demand supports entry.",
                            "citation_ids": ["brain-1"],
                        },
                        {"claim": "Invented.", "citation_ids": ["not-retrieved"]},
                    ]
                }
            ),
            "usage": {"prompt_tokens": 10, "completion_tokens": 10},
            "cost_usd": 0.01,
            "cache_hit": True,
        }

    ledger = BudgetLedger(
        BudgetLimits(max_calls=1, max_tokens=2_000, max_cost_usd=1, max_seconds=5)
    )
    pad = Scratchpad()
    worker = ResearchWorker(
        worker_id="swarm-worker-1",
        grant=WorkerGrant(actor_id="swarm-worker-1"),
        retriever=FixtureRetriever(),
        completion=completion,
    )
    findings = await worker.research(
        question="Should we enter?",
        context={},
        ledger=ledger,
        scratchpad=pad,
        token_allowance=2_000,
        cost_allowance=Decimal("1"),
    )

    assert len(findings) == 1
    assert findings[0].citations[0].object_id == "brain-1"
    assert calls[0]["purpose"] == "chat"
    assert calls[0]["model_policy"] == "cheap"
    assert calls[0]["rag_chunks"]
    assert calls[0]["max_tokens"] > 0
    assert (await ledger.usage()).calls == 1
    assert len(await pad.snapshot()) == 1


def test_worker_grant_cannot_inherit_human_tier() -> None:
    with pytest.raises(ValueError):
        WorkerGrant(actor_id="worker", tier=3)


@pytest.mark.asyncio
async def test_worker_rejects_unaffordable_call_before_provider_dispatch() -> None:
    called = False

    async def completion(*args: Any, **kwargs: Any) -> dict[str, Any]:
        nonlocal called
        called = True
        return {}

    ledger = BudgetLedger(
        BudgetLimits(
            max_calls=1, max_tokens=2_000, max_cost_usd="0.0001", max_seconds=5
        )
    )
    worker = ResearchWorker(
        worker_id="swarm-worker-1",
        grant=WorkerGrant(actor_id="swarm-worker-1"),
        retriever=FixtureRetriever(),
        completion=completion,
    )
    with pytest.raises(BudgetExceededError, match="cost_usd"):
        await worker.research(
            question="Should we enter?",
            context={},
            ledger=ledger,
            scratchpad=Scratchpad(),
            token_allowance=2_000,
            cost_allowance=Decimal("0.0001"),
        )
    assert called is False
    assert ledger.truncated_dimension == "cost_usd"
