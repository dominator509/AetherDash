"""Shared, structurally cited swarm domain models."""

from pydantic import BaseModel, Field, model_validator


class BrainCitation(BaseModel):
    object_id: str = Field(min_length=1)
    provenance_hash: str = Field(min_length=1)


class Finding(BaseModel):
    claim: str = Field(min_length=1)
    citations: tuple[BrainCitation, ...] = Field(min_length=1)

    @model_validator(mode="after")
    def dedupe_citations(self) -> "Finding":
        unique = {
            (citation.object_id, citation.provenance_hash): citation
            for citation in self.citations
        }
        self.citations = tuple(unique.values())
        return self


class ResearchEvidence(BaseModel):
    citation: BrainCitation
    text: str = Field(min_length=1)


class WorkerGrant(BaseModel):
    """A swarm-owned grant. Human session privilege is never represented here."""

    actor_id: str = Field(min_length=1)
    tier: int = Field(default=2, ge=1, le=2)
    scopes: frozenset[str] = frozenset({"brain.recall", "llm.complete"})
