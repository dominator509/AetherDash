"""Exactly-one, proposal-only decision packet convergence."""

from decimal import Decimal
from typing import Literal

from pydantic import BaseModel, Field

from server.swarm.budget import BudgetUsage
from server.swarm.models import BrainCitation, Finding


class DecisionClaim(BaseModel):
    text: str = Field(min_length=1)
    citations: tuple[BrainCitation, ...] = Field(min_length=1)


class DecisionPacket(BaseModel):
    recommendation: DecisionClaim
    confidence: float = Field(ge=0, le=1)
    rationale: tuple[DecisionClaim, ...] = Field(min_length=1)
    citations: tuple[BrainCitation, ...] = Field(min_length=1)
    budget_used: BudgetUsage
    budget_truncated: bool
    truncated_dimension: str | None = None
    proposal_only: Literal[True] = True


def build_packet(
    *,
    question: str,
    findings: tuple[Finding, ...],
    usage: BudgetUsage,
    truncated_dimension: str | None,
) -> DecisionPacket:
    """Converge deterministically so budget exhaustion never requires another call."""
    if findings:
        findings = tuple(
            sorted(
                findings,
                key=lambda finding: (
                    finding.claim,
                    tuple(citation.object_id for citation in finding.citations),
                ),
            )
        )
        claims = tuple(
            DecisionClaim(text=finding.claim, citations=finding.citations)
            for finding in findings
        )
        recommendation = DecisionClaim(
            text=claims[0].text,
            citations=claims[0].citations,
        )
        confidence = min(0.95, Decimal("0.5") + Decimal("0.05") * len(claims))
    else:
        # A cited packet cannot be fabricated without evidence. This path is
        # intentionally rejected by the structural model and handled by the
        # orchestrator as a no-evidence result before emission.
        raise ValueError(f"no cited Brain evidence for question: {question}")

    unique = {
        (citation.object_id, citation.provenance_hash): citation
        for claim in claims
        for citation in claim.citations
    }
    return DecisionPacket(
        recommendation=recommendation,
        confidence=float(confidence),
        rationale=claims,
        citations=tuple(unique.values()),
        budget_used=usage,
        budget_truncated=truncated_dimension is not None,
        truncated_dimension=truncated_dimension,
    )
