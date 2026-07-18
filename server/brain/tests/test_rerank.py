import asyncio
from collections.abc import Sequence

import pytest

from server.brain.recall import ScoredRef
from server.brain.rerank import rerank_with_budget


class FixtureEncoder:
    def __init__(self, scores: dict[str, float], delay: float = 0) -> None:
        self.scores = scores
        self.delay = delay

    async def score(
        self, query: str, candidates: Sequence[tuple[str, str]], *, timeout_s: float
    ) -> dict[str, float]:
        assert query
        assert timeout_s > 0
        if self.delay:
            await asyncio.sleep(self.delay)
        return {
            object_id: self.scores[object_id]
            for object_id, _ in candidates
            if object_id in self.scores
        }


@pytest.mark.asyncio
async def test_cross_encoder_reorders_only_bounded_head() -> None:
    refs = [ScoredRef(str(index), "hash", 1 - index / 10) for index in range(4)]
    documents = {ref.object_id: f"document {ref.object_id}" for ref in refs}

    outcome = await rerank_with_budget(
        "query",
        refs,
        documents,
        FixtureEncoder({"0": 0.1, "1": 0.9}),
        top_m=2,
        timeout_ms=50,
    )

    assert outcome.applied is True
    assert outcome.reason == "applied"
    assert [ref.object_id for ref in outcome.refs] == ["1", "0", "2", "3"]


@pytest.mark.asyncio
async def test_slow_cross_encoder_skips_without_mutating_ranking() -> None:
    refs = [ScoredRef("a", "hash", 1), ScoredRef("b", "hash", 0.5)]
    outcome = await rerank_with_budget(
        "query",
        refs,
        {"a": "alpha", "b": "beta"},
        FixtureEncoder({"a": 0.1, "b": 0.9}, delay=0.05),
        timeout_ms=5,
    )

    assert outcome.applied is False
    assert outcome.reason == "time_cap"
    assert outcome.refs == refs


@pytest.mark.asyncio
async def test_incomplete_or_missing_inputs_skip_fail_closed() -> None:
    refs = [ScoredRef("a", "hash", 1), ScoredRef("b", "hash", 0.5)]
    missing = await rerank_with_budget(
        "query", refs, {"a": "alpha"}, FixtureEncoder({"a": 1}), timeout_ms=20
    )
    incomplete = await rerank_with_budget(
        "query",
        refs,
        {"a": "alpha", "b": "beta"},
        FixtureEncoder({"a": 1}),
        timeout_ms=20,
    )

    assert missing.reason == "documents_missing"
    assert incomplete.reason == "scores_incomplete"
