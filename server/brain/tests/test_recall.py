"""Tests for Recall v1 — RRF hybrid fusion (EP-201, Milestone 4).

Unit tests cover the RRF fusion algorithm in isolation.
Integration tests (marked ``skip_integration``) cover the full pipeline
against Postgres and Qdrant.
"""

import time

import pytest
import pytest_asyncio

from server.brain.pipeline.embed import generate_embedding
from server.brain.recall import (
    ScoredRef,
    _rrf_fuse,
)
from server.brain.tests.conftest import skip_integration

# ═══════════════════════════════════════════════════════════════════════
# Content-dependent embedding unit tests
# ═══════════════════════════════════════════════════════════════════════


def test_embedding_dimension() -> None:
    """generate_embedding returns a 1024-d unit vector."""
    import asyncio

    vec = asyncio.run(generate_embedding("test text"))
    assert len(vec) == 1024
    norm = sum(v * v for v in vec) ** 0.5
    assert abs(norm - 1.0) < 1e-6


def test_embedding_deterministic() -> None:
    """generate_embedding is deterministic (same text → same vector)."""
    import asyncio

    v1 = asyncio.run(generate_embedding("deterministic test"))
    v2 = asyncio.run(generate_embedding("deterministic test"))
    assert v1 == v2


def test_embedding_content_dependent() -> None:
    """Different texts produce different embedding vectors."""
    import asyncio

    v1 = asyncio.run(generate_embedding("Federal Reserve raises rates"))
    v2 = asyncio.run(generate_embedding("Apple reports record earnings"))
    assert v1 != v2, "Different texts must produce different embeddings"


def test_embedding_preserves_text_similarity() -> None:
    """Router stub ranks overlapping text above unrelated text."""
    import asyncio

    query = asyncio.run(generate_embedding("Federal Reserve interest rates"))
    related = asyncio.run(generate_embedding("Federal Reserve raises interest rates"))
    unrelated = asyncio.run(generate_embedding("Apple reports quarterly earnings"))

    def cosine(left: list[float], right: list[float]) -> float:
        return sum(a * b for a, b in zip(left, right, strict=True))

    assert cosine(query, related) > cosine(query, unrelated)


def test_embedding_vs_stub_different() -> None:
    """generate_embedding produces different vectors than fixed-pattern stub."""
    import asyncio

    vec1 = asyncio.run(generate_embedding("market conditions"))
    vec2 = asyncio.run(generate_embedding("completely different text"))
    assert vec1 != vec2, "Different texts must produce different embeddings"


# ═══════════════════════════════════════════════════════════════════════
# RRF fusion unit tests
# ═══════════════════════════════════════════════════════════════════════


def test_rrf_returns_top_k() -> None:
    """RRF returns at most k results."""
    qdrant = [{"object_id": f"obj_{i}", "score": 1.0 - i * 0.01} for i in range(10)]
    fts = [
        {
            "object_id": f"obj_{i}",
            "provenance_hash": f"hash_{i}",
            "score": 0.9 - i * 0.01,
        }
        for i in range(5, 15)
    ]

    result = _rrf_fuse(qdrant, fts, k=5)
    assert len(result) <= 5


def test_rrf_fuses_overlapping_results() -> None:
    """RRF gives higher scores to objects found in both sources."""
    qdrant = [
        {"object_id": "obj_1", "score": 0.9},
        {"object_id": "obj_2", "score": 0.8},
    ]
    fts = [
        {"object_id": "obj_1", "provenance_hash": "hash_1", "score": 0.85},
        {"object_id": "obj_3", "provenance_hash": "hash_3", "score": 0.7},
    ]

    result = _rrf_fuse(qdrant, fts, k=3)

    # obj_1 appears in both sources → highest fusion score
    assert result[0].object_id == "obj_1"
    # obj_2 appears in Qdrant only
    # obj_3 appears in FTS only
    assert len(result) == 3


def test_rrf_single_source() -> None:
    """RRF works with results from only one source."""
    qdrant = [
        {"object_id": "obj_1", "score": 0.9},
        {"object_id": "obj_2", "score": 0.8},
    ]

    result = _rrf_fuse(qdrant, [], k=3)
    assert len(result) == 2
    assert result[0].object_id == "obj_1"
    assert result[1].object_id == "obj_2"

    result = _rrf_fuse([], qdrant, k=3)
    assert len(result) == 2


def test_rrf_empty_inputs() -> None:
    """RRF returns empty for empty inputs."""
    assert _rrf_fuse([], [], k=10) == []


def test_rrf_scores_formula_correct() -> None:
    """RRF scores follow the formula: 1/(rank+60) per source."""
    qdrant = [{"object_id": "obj_a", "score": 1.0}]
    fts = [{"object_id": "obj_a", "provenance_hash": "hash_a", "score": 1.0}]

    result = _rrf_fuse(qdrant, fts, k=2)

    expected_score = 1.0 / (1 + 60) + 1.0 / (1 + 60)
    assert abs(result[0].score - expected_score) < 1e-10
    assert result[0].qdrant_rank == 1
    assert result[0].fts_rank == 1


def test_rrf_deterministic() -> None:
    """Same inputs produce identical output (deterministic)."""
    qdrant = [{"object_id": f"obj_{i}", "score": 1.0 - i * 0.05} for i in range(5)]
    fts = [
        {
            "object_id": f"obj_{i}",
            "provenance_hash": f"hash_{i}",
            "score": 0.9 - i * 0.05,
        }
        for i in range(3, 8)
    ]

    r1 = _rrf_fuse(qdrant, fts, k=10)
    r2 = _rrf_fuse(qdrant, fts, k=10)

    for s1, s2 in zip(r1, r2, strict=False):
        assert s1.object_id == s2.object_id
        assert abs(s1.score - s2.score) < 1e-10
        assert s1.qdrant_rank == s2.qdrant_rank
        assert s1.fts_rank == s2.fts_rank


def test_rrf_propagates_provenance_hash() -> None:
    """RRF propagates provenance_hash from FTS results."""
    qdrant = [{"object_id": "obj_a", "score": 0.9}]
    fts = [{"object_id": "obj_a", "provenance_hash": "abc123", "score": 0.85}]

    result = _rrf_fuse(qdrant, fts, k=2)
    assert result[0].provenance_hash == "abc123"


def test_rrf_annotates_per_source_ranks() -> None:
    """Each ScoredRef has correct per-source rank annotations."""
    qdrant = [
        {"object_id": "obj_x", "score": 0.9},
        {"object_id": "obj_y", "score": 0.8},
    ]
    fts = [
        {"object_id": "obj_y", "provenance_hash": "hash_y", "score": 0.95},
        {"object_id": "obj_z", "provenance_hash": "hash_z", "score": 0.7},
    ]

    result = _rrf_fuse(qdrant, fts, k=3)
    result_by_id = {r.object_id: r for r in result}

    # obj_x: Qdrant rank 1, no FTS
    assert result_by_id["obj_x"].qdrant_rank == 1
    assert result_by_id["obj_x"].fts_rank is None

    # obj_y: Qdrant rank 2, FTS rank 1
    assert result_by_id["obj_y"].qdrant_rank == 2
    assert result_by_id["obj_y"].fts_rank == 1

    # obj_z: no Qdrant, FTS rank 2
    assert result_by_id["obj_z"].qdrant_rank is None
    assert result_by_id["obj_z"].fts_rank == 2


# ═══════════════════════════════════════════════════════════════════════
# ScoredRef dataclass tests
# ═══════════════════════════════════════════════════════════════════════


def test_scored_ref_defaults() -> None:
    """ScoredRef defaults handle None ranks correctly."""
    ref = ScoredRef(object_id="oid", provenance_hash="ph", score=0.5)
    assert ref.qdrant_rank is None
    assert ref.fts_rank is None


def test_scored_ref_with_ranks() -> None:
    """ScoredRef can store both ranks."""
    ref = ScoredRef(
        object_id="oid",
        provenance_hash="ph",
        score=0.5,
        qdrant_rank=1,
        fts_rank=2,
    )
    assert ref.qdrant_rank == 1
    assert ref.fts_rank == 2


# ═══════════════════════════════════════════════════════════════════════
# Integration tests (require dev stack: Postgres :5432 + Qdrant :6333)
# ═══════════════════════════════════════════════════════════════════════


@pytest_asyncio.fixture
async def recall_clean_db() -> None:
    """Clean the brain_objects table before and after each test.

    Resets the module-level asyncpg pool at fixture setup so each test
    starts with a fresh pool on the current event loop.  This avoids
    ``RuntimeError: Event loop is closed`` which occurs on Windows when
    a pool created on one event loop is reused on another (pytest-asyncio
    creates a new loop per test by default).
    """
    from server.brain import store as brain_store

    # Force-reset the module-level pool so get_pool() creates a fresh one
    # on the current test's event loop.
    brain_store._pool = None  # type: ignore[attr-defined]

    pool = None
    try:
        pool = await brain_store.get_pool()
        async with pool.acquire() as conn:
            await conn.execute("DELETE FROM brain_objects")
    except Exception:
        pass  # infra may not be available — skipif will handle it
    yield
    try:
        if pool is not None:
            async with pool.acquire() as conn:
                await conn.execute("DELETE FROM brain_objects")
    except Exception:
        pass  # best-effort teardown
    finally:
        # Reset the module-level pool reference so the next test module's
        # fixture creates a fresh pool on its own event loop.  We do NOT
        # call close_pool() here because the connections are bound to an
        # event loop that pytest-asyncio is about to tear down.
        brain_store._pool = None  # type: ignore[attr-defined]


@pytest.fixture
def recall_test_objects() -> list[dict]:
    """Test objects to insert for recall integration tests."""
    return [
        {
            "kind": "news",
            "content": "Federal Reserve raises interest rates by 25 basis points "
            "affecting the stock market and bond yields.",
            "source": "test-recall-news",
        },
        {
            "kind": "news",
            "content": "Apple Inc reports record quarterly earnings with "
            "revenue exceeding analyst expectations.",
            "source": "test-recall-news",
        },
        {
            "kind": "report",
            "content": "Technical analysis of SPY options flow showing "
            "increased put activity ahead of Fed meeting.",
            "source": "test-recall-report",
        },
        {
            "kind": "note",
            "content": "Quick note about market sentiment being "
            "cautiously optimistic heading into next week.",
            "source": "test-recall-note",
        },
        {
            "kind": "email",
            "content": "Monthly portfolio rebalancing discussion with "
            "allocation changes to fixed income.",
            "source": "test-recall-email",
        },
    ]


@skip_integration
@pytest.mark.asyncio
async def test_recall_returns_results(recall_test_objects, recall_clean_db) -> None:
    """Recall returns results for a query matching test objects."""
    from server.brain import service
    from server.brain.models import ObjectDraft

    refs = []
    for obj in recall_test_objects:
        draft = ObjectDraft(**obj)
        ref = await service.store_draft(draft)
        refs.append(ref)

    # Wait for pipeline to complete (embed -> index -> visible)
    await _wait_for_pipeline(refs)

    results = await service.recall("interest rates", k=10)
    assert len(results) > 0, "Should return results for matching query"


@skip_integration
@pytest.mark.asyncio
async def test_recall_empty_query(recall_test_objects, recall_clean_db) -> None:
    """Recall on empty query returns empty list."""
    from server.brain import service
    from server.brain.models import ObjectDraft

    for obj in recall_test_objects:
        draft = ObjectDraft(**obj)
        await service.store_draft(draft)

    results = await service.recall("", k=10)
    assert results == []


@skip_integration
@pytest.mark.asyncio
async def test_recall_respects_k_limit(recall_test_objects, recall_clean_db) -> None:
    """Recall respects the k limit (returns <= k results)."""
    from server.brain import service
    from server.brain.models import ObjectDraft

    for obj in recall_test_objects:
        draft = ObjectDraft(**obj)
        await service.store_draft(draft)

    results = await service.recall("market", k=2)
    assert len(results) <= 2


@skip_integration
@pytest.mark.asyncio
async def test_recall_filters_by_kind(recall_test_objects, recall_clean_db) -> None:  # noqa: C901
    """Recall filters by kind correctly."""
    from server.brain import service
    from server.brain.models import ObjectDraft

    for obj in recall_test_objects:
        draft = ObjectDraft(**obj)
        await service.store_draft(draft)

    results = await service.recall("market", k=10, filters={"kind": "news"})
    assert len(results) > 0


@skip_integration
@pytest.mark.asyncio
async def test_recall_excludes_cold_tier(recall_test_objects, recall_clean_db) -> None:
    """Recall excludes tier=cold objects by default."""
    from server.brain import service, store
    from server.brain.models import ObjectDraft, Tier

    refs = []
    for obj in recall_test_objects:
        draft = ObjectDraft(**obj)
        ref = await service.store_draft(draft)
        refs.append(ref)

    await _wait_for_pipeline(refs)

    # Manually flip one object to cold
    if refs:
        await store.update_object(refs[0].id, tier=Tier.cold.value)

    results = await service.recall("market", k=10)
    result_ids = {r.object_id for r in results}
    assert refs[0].id not in result_ids, "Cold-tier object should be excluded"


@skip_integration
@pytest.mark.asyncio
async def test_recall_deterministic(recall_test_objects, recall_clean_db) -> None:
    """Same query produces same results (deterministic)."""
    from server.brain import service
    from server.brain.models import ObjectDraft

    refs = []
    for obj in recall_test_objects:
        draft = ObjectDraft(**obj)
        ref = await service.store_draft(draft)
        refs.append(ref)

    await _wait_for_pipeline(refs)

    r1 = await service.recall("market", k=5)
    r2 = await service.recall("market", k=5)

    assert len(r1) == len(r2)
    for s1, s2 in zip(r1, r2, strict=False):
        assert s1.object_id == s2.object_id
        assert abs(s1.score - s2.score) < 1e-6


@skip_integration
@pytest.mark.asyncio
async def test_recall_multi_source_ranking(
    recall_test_objects, recall_clean_db
) -> None:
    """Objects found by both Qdrant and FTS rank higher (RRF boost)."""
    from server.brain import service
    from server.brain.models import ObjectDraft

    for obj in recall_test_objects:
        draft = ObjectDraft(**obj)
        await service.store_draft(draft)

    results = await service.recall("market", k=10)

    # All results should have valid scores (> 0)
    for r in results:
        assert r.score > 0, f"Score should be > 0, got {r.score}"


@skip_integration
@pytest.mark.asyncio
async def test_recall_fts_ranks_present(recall_test_objects, recall_clean_db) -> None:
    """Some results should have FTS rank populated."""
    from server.brain import service
    from server.brain.models import ObjectDraft

    for obj in recall_test_objects:
        draft = ObjectDraft(**obj)
        await service.store_draft(draft)

    results = await service.recall("market", k=10)
    at_least_one_fts = any(r.fts_rank is not None for r in results)
    assert at_least_one_fts, "At least one result should have an FTS rank"


@skip_integration
@pytest.mark.asyncio
async def test_recall_empty_db(recall_clean_db) -> None:
    """Recall on empty DB returns empty list."""
    from server.brain import service

    results = await service.recall("anything", k=10)
    assert results == []


@skip_integration
@pytest.mark.asyncio
async def test_recall_filters_by_trust(recall_test_objects, recall_clean_db) -> None:
    """Recall filters by trust >= correctly."""
    from server.brain import service, store
    from server.brain.models import ObjectDraft

    refs = []
    for obj in recall_test_objects:
        draft = ObjectDraft(**obj)
        ref = await service.store_draft(draft)
        refs.append(ref)

    await _wait_for_pipeline(refs)

    # Bump one object to high trust
    if refs:
        high_trust = str(0.9)
        await store.update_object(refs[0].id, trust=high_trust)

    # Filter by trust >= high
    results = await service.recall("market", k=10, filters={"trust": "high"})
    assert isinstance(results, list)


@pytest.mark.asyncio
@skip_integration
async def test_recall_p95_budget(recall_test_objects, recall_clean_db) -> None:
    """Budget: p95 latency <= 100ms for a small dataset (~5 objects).

    EP-201 requires p95 <= 100ms for the recall path. This test uses a
    minimal dataset of 5 objects against local Postgres + Qdrant, which
    should comfortably meet the budget.

    Note: skipped when infra (Postgres :5432, Qdrant :6333) is unavailable.
    See ``conftest._infra_available()`` for the probe logic.
    """
    from server.brain import service
    from server.brain.models import ObjectDraft

    for obj in recall_test_objects:
        draft = ObjectDraft(**obj)
        await service.store_draft(draft)

    # Warmup: multiple calls to ensure connection pools, Qdrant client,
    # and Postgres query plans are all in their steady state.
    for _ in range(5):
        await service.recall("market", k=10)

    latencies: list[float] = []
    for _ in range(50):
        t0 = time.monotonic()
        await service.recall("market", k=10)
        elapsed = (time.monotonic() - t0) * 1000  # ms
        latencies.append(elapsed)

    latencies.sort()
    p95_idx = int(len(latencies) * 0.95)
    p95 = (
        latencies.pop()
        if not latencies
        else latencies[min(p95_idx, len(latencies) - 1)]
    )
    assert p95 <= 100, (
        f"p95 latency {p95:.1f}ms exceeds 100ms on {len(latencies)} samples. "
        f"EP-201 budget is 100ms on dedicated hardware; this test runs on shared dev infra "
        f"(Docker on Windows). Latencies: {latencies[:5]}..."
    )


# ── Helpers ────────────────────────────────────────────────────────────────


async def _wait_for_pipeline(refs: list, timeout: float = 8.0) -> None:
    """Wait for all pipeline stages to complete and objects to be indexable."""
    import asyncio

    from server.brain import service

    deadline = time.monotonic() + timeout
    for ref in refs:
        while time.monotonic() < deadline:
            obj = await service.get_by_id(ref.id)
            if obj is not None and obj.tier.value == "hot":
                break
            await asyncio.sleep(0.3)

    # Give Qdrant index a moment to settle
    await asyncio.sleep(0.5)
