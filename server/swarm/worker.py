"""Bounded research workers using Brain recall and the EP-202 router only."""

from __future__ import annotations

import json
from collections.abc import Awaitable, Callable
from decimal import Decimal
from typing import Any, Protocol

from pydantic import BaseModel, Field, ValidationError

from server.brain import service as brain_service
from server.llm_router.client import complete
from server.swarm.budget import BudgetExceededError, BudgetLedger
from server.swarm.models import BrainCitation, Finding, ResearchEvidence, WorkerGrant
from server.swarm.scratchpad import Scratchpad


class EvidenceRetriever(Protocol):
    async def recall(self, query: str, *, k: int) -> tuple[ResearchEvidence, ...]: ...


Completion = Callable[..., Awaitable[dict[str, Any]]]


class BrainRecallRetriever:
    async def recall(self, query: str, *, k: int) -> tuple[ResearchEvidence, ...]:
        refs = await brain_service.recall(query, k=k)
        evidence: list[ResearchEvidence] = []
        for ref in refs:
            obj = await brain_service.get_by_id(ref.object_id)
            if obj is None or not obj.summary:
                continue
            evidence.append(
                ResearchEvidence(
                    citation=BrainCitation(
                        object_id=ref.object_id,
                        provenance_hash=ref.provenance_hash,
                    ),
                    text=obj.summary,
                )
            )
        return tuple(evidence)


class _FindingPayload(BaseModel):
    claim: str = Field(min_length=1)
    citation_ids: tuple[str, ...] = Field(min_length=1)


class _WorkerPayload(BaseModel):
    findings: tuple[_FindingPayload, ...] = ()


class ResearchWorker:
    def __init__(
        self,
        *,
        worker_id: str,
        grant: WorkerGrant,
        retriever: EvidenceRetriever,
        completion: Completion = complete,
        recall_k: int = 6,
        max_cost_per_token_usd: Decimal = Decimal("0.0001"),
    ) -> None:
        if grant.actor_id != worker_id:
            raise ValueError("worker grant must belong to the worker")
        self.worker_id = worker_id
        self.grant = grant
        self.retriever = retriever
        self.completion = completion
        self.recall_k = recall_k
        if max_cost_per_token_usd <= 0:
            raise ValueError("the configured token-cost ceiling must be positive")
        self.max_cost_per_token_usd = max_cost_per_token_usd

    async def research(
        self,
        *,
        question: str,
        context: dict[str, Any],
        ledger: BudgetLedger,
        scratchpad: Scratchpad,
        token_allowance: int,
        cost_allowance: Decimal,
    ) -> tuple[Finding, ...]:
        evidence = await self.retriever.recall(question, k=self.recall_k)
        if not evidence:
            return ()

        prior = await scratchpad.snapshot()
        dynamic_inputs = {
            "question": question,
            "context": context,
            "prior_findings": [entry.text for entry in prior],
            "required_output": {
                "findings": [{"claim": "string", "citation_ids": ["object_id"]}]
            },
        }
        chunks = [
            json.dumps(
                {
                    "object_id": item.citation.object_id,
                    "provenance_hash": item.citation.provenance_hash,
                    "summary": item.text,
                },
                separators=(",", ":"),
            )
            for item in evidence
        ]
        # UTF-8 bytes are a deliberately conservative prompt-token upper bound.
        input_reserve = len(
            json.dumps(dynamic_inputs, ensure_ascii=False).encode("utf-8")
        ) + sum(len(chunk.encode("utf-8")) for chunk in chunks)
        affordable_tokens = int(cost_allowance / self.max_cost_per_token_usd)
        effective_allowance = min(token_allowance, affordable_tokens)
        if effective_allowance <= input_reserve:
            await ledger.mark_truncated("cost_usd")
            raise BudgetExceededError("cost_usd")
        output_tokens = effective_allowance - input_reserve
        reservation = await ledger.reserve(
            tokens=input_reserve + output_tokens,
            cost_usd=Decimal(effective_allowance) * self.max_cost_per_token_usd,
        )
        try:
            result = await self.completion(
                "chat",
                dynamic_inputs,
                model_policy="cheap",
                rag_chunks=chunks,
                max_tokens=output_tokens,
                timeout=min(60.0, max(0.1, ledger.remaining_seconds())),
            )
            usage = result.get("usage") or {}
            actual_tokens = int(usage.get("prompt_tokens", 0)) + int(
                usage.get("completion_tokens", 0)
            )
            actual_cost = Decimal(str(result.get("cost_usd", 0)))
            await ledger.commit(
                reservation,
                actual_tokens=actual_tokens,
                actual_cost_usd=actual_cost,
            )
        except BaseException:
            await ledger.abort_call(reservation)
            raise

        if result.get("error"):
            return ()
        findings = self._parse_findings(str(result.get("text", "")), evidence)
        for finding in findings:
            await scratchpad.append(
                self.worker_id,
                finding.claim,
                tuple(citation.object_id for citation in finding.citations),
            )
        return findings

    @staticmethod
    def _parse_findings(
        text: str, evidence: tuple[ResearchEvidence, ...]
    ) -> tuple[Finding, ...]:
        try:
            payload = _WorkerPayload.model_validate_json(text)
        except (ValidationError, ValueError):
            return ()
        citations = {item.citation.object_id: item.citation for item in evidence}
        findings: list[Finding] = []
        for item in payload.findings:
            attached = tuple(
                citations[citation_id]
                for citation_id in dict.fromkeys(item.citation_ids)
                if citation_id in citations
            )
            if attached:
                findings.append(Finding(claim=item.claim, citations=attached))
        return tuple(findings)
