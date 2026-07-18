"""Bounded Bayesian source reliability from outcomes and operator feedback."""

from dataclasses import dataclass
from datetime import UTC, datetime
from typing import Protocol

import asyncpg
import ulid


@dataclass(frozen=True)
class ReliabilityEvidence:
    correlated_moves: int = 0
    uncorrelated_moves: int = 0
    positive_feedback: int = 0
    negative_feedback: int = 0

    def __post_init__(self) -> None:
        if (
            min(
                self.correlated_moves,
                self.uncorrelated_moves,
                self.positive_feedback,
                self.negative_feedback,
            )
            < 0
        ):
            raise ValueError("reliability evidence counts cannot be negative")

    @property
    def count(self) -> int:
        return (
            self.correlated_moves
            + self.uncorrelated_moves
            + self.positive_feedback
            + self.negative_feedback
        )


def reliability_score(evidence: ReliabilityEvidence) -> float:
    """Return a [0,1] posterior with a neutral four-observation prior.

    Operator feedback has weight two because it is an explicit human quality
    judgment; market-correlation observations each have weight one.
    """
    positive = 2 + evidence.correlated_moves + 2 * evidence.positive_feedback
    total = (
        4
        + evidence.correlated_moves
        + evidence.uncorrelated_moves
        + 2 * (evidence.positive_feedback + evidence.negative_feedback)
    )
    return max(0.0, min(1.0, positive / total))


class PostgresReliabilityEvidence:
    def __init__(self, pool: asyncpg.Pool) -> None:
        self.pool = pool

    async def record_correlation(
        self, *, source: str, object_id: str, correlated: bool
    ) -> str:
        evidence_id = str(ulid.new())
        await self.pool.execute(
            """
            INSERT INTO ingest_source_reliability_evidence
                (id,source,evidence_kind,positive,object_id)
            VALUES ($1,$2,'correlation',$3,$4)
            """,
            evidence_id,
            source,
            correlated,
            object_id,
        )
        return evidence_id

    async def record_feedback(
        self,
        *,
        source: str,
        actor_id: str,
        positive: bool,
        reason: str | None = None,
    ) -> str:
        evidence_id = str(ulid.new())
        await self.pool.execute(
            """
            INSERT INTO ingest_source_reliability_evidence
                (id,source,evidence_kind,positive,actor_id,reason)
            VALUES ($1,$2,'feedback',$3,$4,$5)
            """,
            evidence_id,
            source,
            positive,
            actor_id,
            reason,
        )
        return evidence_id

    async def summary(self, source: str) -> ReliabilityEvidence:
        row = await self.pool.fetchrow(
            """
            SELECT
              count(*) FILTER (WHERE evidence_kind='correlation' AND positive) AS correlated,
              count(*) FILTER (WHERE evidence_kind='correlation' AND NOT positive) AS uncorrelated,
              count(*) FILTER (WHERE evidence_kind='feedback' AND positive) AS positive_feedback,
              count(*) FILTER (WHERE evidence_kind='feedback' AND NOT positive) AS negative_feedback
            FROM ingest_source_reliability_evidence WHERE source=$1
            """,
            source,
        )
        return ReliabilityEvidence(
            correlated_moves=row["correlated"],
            uncorrelated_moves=row["uncorrelated"],
            positive_feedback=row["positive_feedback"],
            negative_feedback=row["negative_feedback"],
        )


def _ensure_source_schema(connection: object) -> None:
    connection.execute(
        "CREATE NODE TABLE IF NOT EXISTS Source ("
        "id STRING, uri STRING, reliability DOUBLE DEFAULT 0.5, "
        "evidence_count INT64 DEFAULT 0, reliability_updated_ts TIMESTAMP, "
        "PRIMARY KEY (id))"
    )
    connection.execute(
        "ALTER TABLE Source ADD IF NOT EXISTS reliability DOUBLE DEFAULT 0.5"
    )
    connection.execute(
        "ALTER TABLE Source ADD IF NOT EXISTS evidence_count INT64 DEFAULT 0"
    )
    connection.execute(
        "ALTER TABLE Source ADD IF NOT EXISTS reliability_updated_ts TIMESTAMP"
    )


class KuzuReliabilityWriter:
    def __init__(self, connection: object) -> None:
        self.connection = connection

    def write(self, source: str, evidence: ReliabilityEvidence) -> float:
        _ensure_source_schema(self.connection)
        score = reliability_score(evidence)
        self.connection.execute(
            "MERGE (s:Source {id: $id}) "
            "SET s.uri=$uri, s.reliability=$score, s.evidence_count=$count, "
            "s.reliability_updated_ts=$updated",
            {
                "id": f"source_{source}",
                "uri": source,
                "score": score,
                "count": evidence.count,
                "updated": datetime.now(UTC).replace(tzinfo=None),
            },
        )
        return score


class EvidenceRepository(Protocol):
    async def summary(self, source: str) -> ReliabilityEvidence: ...


class ReliabilityWriter(Protocol):
    def write(self, source: str, evidence: ReliabilityEvidence) -> float: ...


class ReliabilityService:
    def __init__(
        self, repository: EvidenceRepository, writer: ReliabilityWriter
    ) -> None:
        self.repository = repository
        self.writer = writer

    async def refresh(self, source: str) -> float:
        evidence = await self.repository.summary(source)
        return self.writer.write(source, evidence)
