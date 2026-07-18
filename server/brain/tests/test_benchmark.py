from pathlib import Path

import pytest

from server.brain.benchmark import (
    evaluate,
    fixture_rerank_ranker,
    graph_ranker,
    load_cases,
    ndcg_at_k,
    reciprocal_rank,
    v1_ranker,
    weighted_graph_ranker,
)

DATASET = Path("testdata/brain-bench/graded")


def test_graded_metrics_have_known_values() -> None:
    relevance = {"best": 3, "useful": 1, "noise": 0}
    assert ndcg_at_k(["best", "useful", "noise"], relevance, 3) == 1.0
    assert ndcg_at_k(["noise", "useful", "best"], relevance, 3) < 1.0
    assert reciprocal_rank(["noise", "useful", "best"], relevance) == 0.5


def test_dataset_is_graded_and_v1_baseline_is_reproducible() -> None:
    cases = load_cases(DATASET)
    first = evaluate(cases, v1_ranker, k=5, timing_repetitions=2)
    second = evaluate(cases, v1_ranker, k=5, timing_repetitions=2)

    assert first.queries == 4
    assert first.ndcg_at_k == pytest.approx(second.ndcg_at_k)
    assert first.mrr == pytest.approx(second.mrr)
    assert first.ndcg_at_k >= 0.8
    assert first.mrr == 1.0
    assert first.ranking_latency_p95_ms < 100


def test_graph_expansion_does_not_regress_graded_baseline() -> None:
    cases = load_cases(DATASET)
    baseline = evaluate(cases, v1_ranker, k=5, timing_repetitions=2)
    expanded = evaluate(cases, graph_ranker, k=5, timing_repetitions=2)

    assert expanded.ndcg_at_k >= baseline.ndcg_at_k
    assert expanded.mrr >= baseline.mrr
    assert expanded.ranking_latency_p95_ms < 100


def test_decay_and_reliability_improve_or_preserve_graph_ranking() -> None:
    cases = load_cases(DATASET)
    expanded = evaluate(cases, graph_ranker, k=5, timing_repetitions=2)
    weighted = evaluate(cases, weighted_graph_ranker, k=5, timing_repetitions=2)

    assert weighted.ndcg_at_k >= expanded.ndcg_at_k
    assert weighted.mrr >= expanded.mrr
    assert weighted.ranking_latency_p95_ms < 100


def test_fixture_cross_encoder_improves_graded_ranking() -> None:
    cases = load_cases(DATASET)
    weighted = evaluate(cases, weighted_graph_ranker, k=5, timing_repetitions=2)
    reranked = evaluate(cases, fixture_rerank_ranker, k=5, timing_repetitions=2)

    assert reranked.ndcg_at_k > weighted.ndcg_at_k
    assert reranked.mrr >= weighted.mrr
    assert reranked.ranking_latency_p95_ms < 100


def test_dataset_rejects_missing_qrels(tmp_path: Path) -> None:
    (tmp_path / "queries.json").write_text(
        '[{"id":"missing","query":"x","qdrant":[],"fts":[]}]',
        encoding="utf-8",
    )
    (tmp_path / "qrels.json").write_text("{}", encoding="utf-8")
    with pytest.raises(ValueError, match="relevance judgments"):
        load_cases(tmp_path)
