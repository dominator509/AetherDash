"""Budget-bounded optional cross-encoder reranking through EP-202."""

import asyncio
import json
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from typing import Protocol

from server.brain.recall import ScoredRef


class CrossEncoder(Protocol):
    async def score(
        self, query: str, candidates: Sequence[tuple[str, str]], *, timeout_s: float
    ) -> Mapping[str, float]: ...


class RouterCrossEncoder:
    """Use the cache-first EP-202 router with a local-only model policy."""

    async def score(
        self, query: str, candidates: Sequence[tuple[str, str]], *, timeout_s: float
    ) -> Mapping[str, float]:
        from server.llm_router.client import complete  # noqa: PLC0415

        response = await complete(
            "extract",
            {
                "task": "cross_encoder_rerank",
                "query": query,
                "candidates": [
                    {"id": object_id, "text": text[:2_000]}
                    for object_id, text in candidates
                ],
                "response_schema": {"scores": {"object_id": "number_0_to_1"}},
            },
            model_policy="local",
            static_context_ref="brain-recall-cross-encoder-v1",
            timeout=timeout_s,
        )
        if response.get("error"):
            raise RuntimeError("rerank router unavailable")
        payload = json.loads(response.get("text", ""))
        scores = payload.get("scores") if isinstance(payload, dict) else None
        if not isinstance(scores, dict):
            raise ValueError("rerank response does not contain a score mapping")
        allowed = {object_id for object_id, _ in candidates}
        return {
            str(object_id): min(1.0, max(0.0, float(score)))
            for object_id, score in scores.items()
            if str(object_id) in allowed
        }


@dataclass(frozen=True)
class RerankOutcome:
    refs: list[ScoredRef]
    applied: bool
    reason: str


async def rerank_with_budget(
    query: str,
    refs: list[ScoredRef],
    documents: Mapping[str, str],
    encoder: CrossEncoder,
    *,
    top_m: int = 12,
    timeout_ms: float = 25.0,
) -> RerankOutcome:
    if top_m < 1 or timeout_ms <= 0:
        return RerankOutcome(
            refs=list(refs), applied=False, reason="budget_unavailable"
        )
    candidates = [
        (ref.object_id, documents.get(ref.object_id, "")) for ref in refs[:top_m]
    ]
    if not candidates or any(not text.strip() for _, text in candidates):
        return RerankOutcome(refs=list(refs), applied=False, reason="documents_missing")
    try:
        scores = await asyncio.wait_for(
            encoder.score(query, candidates, timeout_s=timeout_ms / 1_000),
            timeout=timeout_ms / 1_000,
        )
    except TimeoutError:
        return RerankOutcome(refs=list(refs), applied=False, reason="time_cap")
    except (RuntimeError, ValueError, json.JSONDecodeError, TypeError):
        return RerankOutcome(refs=list(refs), applied=False, reason="model_error")
    if len(scores) != len(candidates):
        return RerankOutcome(refs=list(refs), applied=False, reason="scores_incomplete")

    original_rank = {ref.object_id: rank for rank, ref in enumerate(refs)}
    head = list(refs[:top_m])
    for ref in head:
        ref.rerank_score = scores[ref.object_id]
    head.sort(key=lambda ref: (-scores[ref.object_id], original_rank[ref.object_id]))
    return RerankOutcome(refs=[*head, *refs[top_m:]], applied=True, reason="applied")
