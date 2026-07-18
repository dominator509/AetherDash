"""Versioned graded-relevance benchmark for Brain recall rankers."""

import argparse
import json
import math
import statistics
import time
from collections.abc import Callable, Sequence
from dataclasses import dataclass
from datetime import UTC, datetime, timedelta
from pathlib import Path
from typing import Any

from server.brain.recall import (
    RecallMetadata,
    ScoredRef,
    _apply_decay_and_reliability,
    _rrf_fuse,
    _rrf_fuse_with_graph,
)


@dataclass(frozen=True)
class BenchmarkCase:
    query_id: str
    query: str
    qdrant: tuple[dict[str, Any], ...]
    fts: tuple[dict[str, Any], ...]
    graph: tuple[dict[str, Any], ...]
    metadata: dict[str, dict[str, Any]]
    rerank_scores: dict[str, float]
    relevance: dict[str, int]


@dataclass(frozen=True)
class BenchmarkReport:
    queries: int
    ndcg_at_k: float
    mrr: float
    ranking_latency_p95_ms: float

    def as_dict(self) -> dict[str, int | float]:
        return {
            "queries": self.queries,
            "ndcg_at_k": round(self.ndcg_at_k, 6),
            "mrr": round(self.mrr, 6),
            "ranking_latency_p95_ms": round(self.ranking_latency_p95_ms, 6),
        }


Ranker = Callable[[BenchmarkCase, int], Sequence[ScoredRef]]


def load_cases(dataset: str | Path) -> list[BenchmarkCase]:
    root = Path(dataset)
    queries = json.loads((root / "queries.json").read_text(encoding="utf-8"))
    qrels = json.loads((root / "qrels.json").read_text(encoding="utf-8"))
    if not isinstance(queries, list) or not isinstance(qrels, dict):
        raise ValueError("graded benchmark files have an invalid top-level shape")

    cases: list[BenchmarkCase] = []
    seen: set[str] = set()
    for raw in queries:
        query_id = raw["id"]
        if query_id in seen or query_id not in qrels:
            raise ValueError(
                "every unique benchmark query requires relevance judgments"
            )
        seen.add(query_id)
        relevance = {str(key): int(value) for key, value in qrels[query_id].items()}
        if not relevance or any(grade < 0 or grade > 3 for grade in relevance.values()):
            raise ValueError("relevance grades must be integers in [0,3]")
        cases.append(
            BenchmarkCase(
                query_id=query_id,
                query=raw["query"],
                qdrant=tuple(raw["qdrant"]),
                fts=tuple(raw["fts"]),
                graph=tuple(raw.get("graph", [])),
                metadata=dict(raw.get("metadata", {})),
                rerank_scores={
                    str(key): float(value)
                    for key, value in raw.get("rerank_scores", {}).items()
                },
                relevance=relevance,
            )
        )
    if not cases:
        raise ValueError("graded benchmark must contain at least one query")
    return cases


def ndcg_at_k(ranked_ids: Sequence[str], relevance: dict[str, int], k: int) -> float:
    def dcg(grades: Sequence[int]) -> float:
        return sum(
            (2**grade - 1) / math.log2(rank + 2) for rank, grade in enumerate(grades)
        )

    actual = [relevance.get(object_id, 0) for object_id in ranked_ids[:k]]
    ideal = sorted(relevance.values(), reverse=True)[:k]
    ideal_score = dcg(ideal)
    return dcg(actual) / ideal_score if ideal_score else 0.0


def reciprocal_rank(ranked_ids: Sequence[str], relevance: dict[str, int]) -> float:
    for rank, object_id in enumerate(ranked_ids, start=1):
        if relevance.get(object_id, 0) > 0:
            return 1.0 / rank
    return 0.0


def v1_ranker(case: BenchmarkCase, k: int) -> list[ScoredRef]:
    return _rrf_fuse(list(case.qdrant), list(case.fts), k)


def graph_ranker(case: BenchmarkCase, k: int) -> list[ScoredRef]:
    return _rrf_fuse_with_graph(list(case.qdrant), list(case.fts), list(case.graph), k)


def evaluate(
    cases: Sequence[BenchmarkCase],
    ranker: Ranker,
    *,
    k: int = 10,
    timing_repetitions: int = 25,
) -> BenchmarkReport:
    if k < 1 or timing_repetitions < 1:
        raise ValueError("k and timing_repetitions must be positive")

    ndcg_scores: list[float] = []
    reciprocal_ranks: list[float] = []
    latencies_ms: list[float] = []
    for case in cases:
        ranked = ranker(case, k)
        ranked_ids = [ref.object_id for ref in ranked]
        ndcg_scores.append(ndcg_at_k(ranked_ids, case.relevance, k))
        reciprocal_ranks.append(reciprocal_rank(ranked_ids, case.relevance))
        for _ in range(timing_repetitions):
            started = time.perf_counter()
            ranker(case, k)
            latencies_ms.append((time.perf_counter() - started) * 1_000)

    ordered_latency = sorted(latencies_ms)
    p95_index = max(0, math.ceil(len(ordered_latency) * 0.95) - 1)
    return BenchmarkReport(
        queries=len(cases),
        ndcg_at_k=statistics.fmean(ndcg_scores),
        mrr=statistics.fmean(reciprocal_ranks),
        ranking_latency_p95_ms=ordered_latency[p95_index],
    )


def weighted_graph_ranker(case: BenchmarkCase, k: int) -> list[ScoredRef]:
    now = datetime(2026, 7, 18, tzinfo=UTC)
    metadata = {
        object_id: RecallMetadata(
            kind=value["kind"],
            ingested_ts=now - timedelta(hours=float(value["age_hours"])),
            source_reliability=float(value["reliability"]),
        )
        for object_id, value in case.metadata.items()
    }
    return _apply_decay_and_reliability(graph_ranker(case, k), metadata, now=now)


def fixture_rerank_ranker(case: BenchmarkCase, k: int) -> list[ScoredRef]:
    ranked = weighted_graph_ranker(case, k)
    original_rank = {ref.object_id: rank for rank, ref in enumerate(ranked)}
    ranked.sort(
        key=lambda ref: (
            -case.rerank_scores.get(ref.object_id, -1.0),
            original_rank[ref.object_id],
        )
    )
    return ranked


def main() -> None:
    parser = argparse.ArgumentParser(description="Run the Brain graded benchmark")
    parser.add_argument("dataset", type=Path)
    parser.add_argument("--k", type=int, default=10)
    parser.add_argument("--timing-repetitions", type=int, default=25)
    parser.add_argument(
        "--ranker",
        choices=("v1", "graph", "weighted", "rerank"),
        default="v1",
    )
    args = parser.parse_args()
    report = evaluate(
        load_cases(args.dataset),
        {
            "v1": v1_ranker,
            "graph": graph_ranker,
            "weighted": weighted_graph_ranker,
            "rerank": fixture_rerank_ranker,
        }[args.ranker],
        k=args.k,
        timing_repetitions=args.timing_repetitions,
    )
    print(json.dumps(report.as_dict(), sort_keys=True))


if __name__ == "__main__":
    main()
