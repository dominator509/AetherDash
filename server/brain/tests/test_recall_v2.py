import asyncio
import time

import pytest

from server.brain import recall as recall_module
from server.brain.recall import ScoredRef


def _seed() -> list[ScoredRef]:
    return [ScoredRef("seed", "hash", 1.0, qdrant_rank=1, fts_rank=1)]


@pytest.mark.asyncio
async def test_budget_breaker_returns_unmodified_v1_results(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    async def fake_v1(query: str, k: int, filters: dict | None):
        return _seed()

    async def slow_v2(query: str, refs: list[ScoredRef], filters: dict, *, k: int):
        await asyncio.sleep(0.1)
        return []

    monkeypatch.setattr(recall_module, "recall_v1", fake_v1)
    monkeypatch.setattr(recall_module, "_enhance_v2", slow_v2)
    monkeypatch.setenv("AETHER_BRAIN__RECALL_V2", "1")
    monkeypatch.setenv("AETHER_BRAIN__RECALL_BUDGET_MS", "5")

    started = time.perf_counter()
    result = await recall_module.recall("query", 1)
    elapsed_ms = (time.perf_counter() - started) * 1_000

    assert [ref.object_id for ref in result] == ["seed"]
    assert result[0].recall_path == "v1_budget_fallback"
    assert elapsed_ms < 50


@pytest.mark.asyncio
async def test_v2_error_returns_v1_and_disabled_v2_never_enhances(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    calls = 0

    async def fake_v1(query: str, k: int, filters: dict | None):
        return _seed()

    async def broken_v2(query: str, refs: list[ScoredRef], filters: dict, *, k: int):
        nonlocal calls
        calls += 1
        raise RuntimeError("fixture failure")

    monkeypatch.setattr(recall_module, "recall_v1", fake_v1)
    monkeypatch.setattr(recall_module, "_enhance_v2", broken_v2)
    monkeypatch.setenv("AETHER_BRAIN__RECALL_V2", "1")
    result = await recall_module.recall("query", 1)
    assert result[0].recall_path == "v1_error_fallback"
    assert calls == 1

    monkeypatch.setenv("AETHER_BRAIN__RECALL_V2", "0")
    result = await recall_module.recall("query", 1)
    assert result[0].recall_path == "v1"
    assert calls == 1
