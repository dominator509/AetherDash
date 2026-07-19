import asyncio
import json
from decimal import Decimal
from typing import Any

import pytest
from pydantic import ValidationError

from server.swarm.budget import BudgetLimits
from server.swarm.models import BrainCitation, ResearchEvidence
from server.swarm.orchestrator import ProgressEvent, SwarmOrchestrator, SwarmRequest
from server.swarm.packet import DecisionClaim


class FixtureRetriever:
    async def recall(self, query: str, *, k: int) -> tuple[ResearchEvidence, ...]:
        return (
            ResearchEvidence(
                citation=BrainCitation(object_id="brain-1", provenance_hash="hash-1"),
                text="Fixture evidence.",
            ),
        )


def request(*, workers: int = 3, **budget: object) -> SwarmRequest:
    values: dict[str, object] = {
        "max_calls": 3,
        "max_tokens": 6_000,
        "max_cost_usd": 3,
        "max_seconds": 5,
    }
    values.update(budget)
    return SwarmRequest(
        question="Decide", budget=BudgetLimits(**values), workers=workers
    )


@pytest.mark.asyncio
async def test_launch_returns_exactly_one_cited_proposal_packet() -> None:
    async def completion(*args: Any, **kwargs: Any) -> dict[str, Any]:
        return {
            "text": json.dumps(
                {
                    "findings": [
                        {"claim": "Proceed cautiously.", "citation_ids": ["brain-1"]}
                    ]
                }
            ),
            "usage": {"prompt_tokens": 10, "completion_tokens": 10},
            "cost_usd": 0.01,
            "cache_hit": True,
        }

    events: list[ProgressEvent] = []

    async def progress(event: ProgressEvent) -> None:
        events.append(event)

    packet = await SwarmOrchestrator(
        retriever=FixtureRetriever(), completion=completion
    ).launch(request(), progress=progress)

    assert packet.proposal_only is True
    assert packet.recommendation.citations[0].provenance_hash == "hash-1"
    assert len([event for event in events if event.kind == "packet_ready"]) == 1
    assert packet.budget_used.calls == 3


@pytest.mark.asyncio
async def test_call_budget_truncates_gracefully_without_overspend() -> None:
    async def completion(*args: Any, **kwargs: Any) -> dict[str, Any]:
        await asyncio.sleep(0)
        return {
            "text": json.dumps(
                {"findings": [{"claim": "Cited result.", "citation_ids": ["brain-1"]}]}
            ),
            "usage": {"prompt_tokens": 1, "completion_tokens": 1},
            "cost_usd": 0,
        }

    packet = await SwarmOrchestrator(
        retriever=FixtureRetriever(), completion=completion
    ).launch(request(max_calls=1))
    assert packet.budget_used.calls == 1
    assert packet.budget_truncated is True
    assert packet.truncated_dimension == "calls"
    assert packet.budget_used.calls <= 1


@pytest.mark.asyncio
async def test_timeout_returns_partial_cited_packet_and_counts_aborted_call() -> None:
    calls = 0

    async def completion(*args: Any, **kwargs: Any) -> dict[str, Any]:
        nonlocal calls
        calls += 1
        if calls == 2:
            await asyncio.sleep(1)
        return {
            "text": json.dumps(
                {
                    "findings": [
                        {"claim": f"Finding {calls}.", "citation_ids": ["brain-1"]}
                    ]
                }
            ),
            "usage": {"prompt_tokens": 1, "completion_tokens": 1},
            "cost_usd": 0,
        }

    packet = await SwarmOrchestrator(
        retriever=FixtureRetriever(), completion=completion
    ).launch(request(max_calls=2, max_seconds=0.05))
    assert packet.budget_truncated is True
    assert packet.truncated_dimension == "seconds"
    assert packet.budget_used.calls == 2
    assert packet.recommendation.citations


@pytest.mark.asyncio
async def test_provider_accounting_overage_truncates_instead_of_crashing() -> None:
    calls = 0

    async def completion(*args: Any, **kwargs: Any) -> dict[str, Any]:
        nonlocal calls
        calls += 1
        return {
            "text": json.dumps(
                {
                    "findings": [
                        {"claim": f"Finding {calls}.", "citation_ids": ["brain-1"]}
                    ]
                }
            ),
            "usage": {"prompt_tokens": 1, "completion_tokens": 1},
            "cost_usd": "0.01" if calls == 1 else "2",
        }

    packet = await SwarmOrchestrator(
        retriever=FixtureRetriever(), completion=completion
    ).launch(request(max_calls=2, max_cost_usd=Decimal("1"), workers=2))
    assert packet.budget_truncated is True
    assert packet.truncated_dimension == "provider_overage"
    assert packet.budget_used.calls == 2
    assert packet.budget_used.cost_usd <= Decimal("1")


@pytest.mark.asyncio
async def test_later_wave_receives_first_wave_scratchpad_findings() -> None:
    prior_findings: list[list[str]] = []

    async def completion(
        purpose: str, dynamic_inputs: dict[str, Any], **kwargs: Any
    ) -> dict[str, Any]:
        prior_findings.append(list(dynamic_inputs["prior_findings"]))
        return {
            "text": json.dumps(
                {
                    "findings": [
                        {
                            "claim": f"Finding {len(prior_findings)}.",
                            "citation_ids": ["brain-1"],
                        }
                    ]
                }
            ),
            "usage": {"prompt_tokens": 1, "completion_tokens": 1},
            "cost_usd": 0,
        }

    await SwarmOrchestrator(retriever=FixtureRetriever(), completion=completion).launch(
        request(max_calls=3)
    )
    assert prior_findings[0] == []
    assert prior_findings[-1]


def test_uncited_claim_is_structurally_rejected() -> None:
    with pytest.raises(ValidationError):
        DecisionClaim(text="unsupported", citations=())
