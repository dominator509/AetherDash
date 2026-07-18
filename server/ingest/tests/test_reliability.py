from pathlib import Path

import kuzu
import pytest

from server.ingest.scoring.reliability import (
    KuzuReliabilityWriter,
    ReliabilityEvidence,
    ReliabilityService,
    reliability_score,
)


def test_reliability_score_is_bounded_and_neutral_without_evidence() -> None:
    assert reliability_score(ReliabilityEvidence()) == 0.5
    assert 0 <= reliability_score(ReliabilityEvidence(correlated_moves=10_000)) <= 1
    assert 0 <= reliability_score(ReliabilityEvidence(negative_feedback=10_000)) <= 1


def test_reliability_score_is_monotonic_for_positive_and_negative_evidence() -> None:
    baseline = ReliabilityEvidence(correlated_moves=3, uncorrelated_moves=2)
    score = reliability_score(baseline)
    assert (
        reliability_score(ReliabilityEvidence(correlated_moves=4, uncorrelated_moves=2))
        > score
    )
    assert (
        reliability_score(ReliabilityEvidence(correlated_moves=3, uncorrelated_moves=3))
        < score
    )
    assert (
        reliability_score(
            ReliabilityEvidence(
                correlated_moves=3, uncorrelated_moves=2, positive_feedback=1
            )
        )
        > score
    )
    assert (
        reliability_score(
            ReliabilityEvidence(
                correlated_moves=3, uncorrelated_moves=2, negative_feedback=1
            )
        )
        < score
    )


def test_negative_evidence_counts_are_rejected() -> None:
    with pytest.raises(ValueError, match="cannot be negative"):
        ReliabilityEvidence(correlated_moves=-1)


def test_kuzu_writer_upgrades_existing_source_schema_and_persists_score(
    tmp_path: Path,
) -> None:
    database = kuzu.Database(str(tmp_path / "reliability.kuzu"))
    connection = kuzu.Connection(database)
    connection.execute(
        "CREATE NODE TABLE Source (id STRING, uri STRING, PRIMARY KEY (id))"
    )
    writer = KuzuReliabilityWriter(connection)
    evidence = ReliabilityEvidence(
        correlated_moves=4,
        uncorrelated_moves=1,
        positive_feedback=2,
        negative_feedback=1,
    )

    score = writer.write("official:fixture", evidence)
    result = connection.execute(
        "MATCH (s:Source {id: $id}) RETURN s.uri,s.reliability,s.evidence_count",
        {"id": "source_official:fixture"},
    ).get_next()

    assert result[0] == "official:fixture"
    assert result[1] == pytest.approx(score)
    assert result[2] == evidence.count


class FakeRepository:
    async def summary(self, source: str) -> ReliabilityEvidence:
        assert source == "official:fixture"
        return ReliabilityEvidence(correlated_moves=2)


class FakeWriter:
    def __init__(self) -> None:
        self.written: tuple[str, ReliabilityEvidence] | None = None

    def write(self, source: str, evidence: ReliabilityEvidence) -> float:
        self.written = (source, evidence)
        return reliability_score(evidence)


@pytest.mark.asyncio
async def test_reliability_service_projects_repository_summary() -> None:
    writer = FakeWriter()
    service = ReliabilityService(FakeRepository(), writer)
    score = await service.refresh("official:fixture")
    assert score > 0.5
    assert writer.written == (
        "official:fixture",
        ReliabilityEvidence(correlated_moves=2),
    )
