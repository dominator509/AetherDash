"""Explain tree assembly — deterministic joins over stored data.

SPEC-011 contract:
    Brain.Explain(opportunity_id) -> ExplainTree
    Assembly is deterministic joins over stored data:
    opportunity -> scoring inputs -> evidence objects (refs + extractor spans) -> provenance links
    Plain-language layer comes from stored summaries (not fresh LLM generation).
    Fresh generation is a copilot action, tier-gated.
"""

import logging

from server.brain import store as brain_store
from server.brain.models import BrainObject, ObjectKind, TrustLevel

logger = logging.getLogger(__name__)

# ---------------------------------------------------------------------------
# Kuzu graph lookup (best-effort, stub if unavailable)
# ---------------------------------------------------------------------------

_KUZU_AVAILABLE: bool = False
try:
    import kuzu  # noqa: F401

    _KUZU_AVAILABLE = True
except ImportError:
    logger.debug("explain: kuzu-py not available — graph linking stubbed")


def _query_kuzu_linked_events(event_id: str) -> list[dict]:
    """Query Kuzu for linked events and entities.

    Returns list of {event_id, entity_name, edge_type} dicts.
    Stub: returns empty list if Kuzu is unavailable or query fails.
    """
    if not _KUZU_AVAILABLE:
        return []
    try:
        import kuzu  # noqa: PLC0415

        db = kuzu.Database("./data/kuzu")
        conn = kuzu.Connection(db)
        results = conn.execute(
            "MATCH (e:Event)-[r:MENTIONS]->(en:Entity) "
            "WHERE e.id = $1 "
            "RETURN e.id, en.name, r._label",
            [event_id],
        )
        linked: list[dict] = []
        while results.has_next():
            row = results.get_next()
            linked.append(
                {
                    "event_id": row[0],
                    "entity_name": row[1],
                    "edge_type": row[2],
                }
            )
        return linked
    except Exception as exc:
        logger.debug("explain: Kuzu query failed (%s) — returning empty", exc)
        return []


# ---------------------------------------------------------------------------
# Scoring inputs
# ---------------------------------------------------------------------------


def _build_scoring_inputs(obj: BrainObject) -> list[dict]:
    """Build scoring_inputs list from the BrainObject fields.

    Each input carries a ``name``, ``value``, and ``source`` describing
    where the value came from.
    """
    inputs: list[dict] = []

    # Trust level is a primary scoring input
    trust_val = obj.trust.value if isinstance(obj.trust, TrustLevel) else str(obj.trust)
    inputs.append({"name": "trust", "value": trust_val, "source": obj.source})

    # Confidence from the extraction pipeline (if computed)
    if obj.confidence is not None:
        inputs.append(
            {
                "name": "confidence",
                "value": obj.confidence,
                "source": "pipeline/extract",
            }
        )

    # Object kind provides context for scoring
    kind_val = obj.kind.value if isinstance(obj.kind, ObjectKind) else str(obj.kind)
    inputs.append({"name": "kind", "value": kind_val, "source": obj.source})

    # Extracted entities (limit to 10 for readability)
    if obj.entities:
        inputs.append(
            {
                "name": "entities",
                "value": obj.entities[:10],
                "source": "pipeline/extract",
            }
        )

    # Market keys from the link stage
    if obj.market_keys:
        inputs.append(
            {
                "name": "market_keys",
                "value": obj.market_keys,
                "source": "pipeline/link",
            }
        )

    return inputs


# ---------------------------------------------------------------------------
# Evidence assembly
# ---------------------------------------------------------------------------


async def _build_evidence(obj: BrainObject) -> list[dict]:
    """Build evidence list from Kuzu-linked events and object metadata.

    Each evidence entry has:
      - ref: brain_ref or event_id
      - span: relevant excerpt or entity name
      - provenance: where this evidence came from

    Evidence sources (in priority order):
    1. Kuzu graph MENTIONS edges
    2. ``linked_events`` stored on the BrainObject
    """
    evidence: list[dict] = []

    # 1. Kuzu-linked events
    linked = _query_kuzu_linked_events(f"event_{obj.id}")
    seen_refs: set[str] = set()
    for link in linked:
        ref = link["event_id"]
        if ref not in seen_refs:
            seen_refs.add(ref)
            evidence.append(
                {
                    "ref": ref,
                    "span": link["entity_name"],
                    "provenance": f"kuzu:{link['edge_type']}",
                }
            )

    # 2. linked_events from the BrainObject itself
    for event_id in obj.linked_events:
        if event_id not in seen_refs:
            seen_refs.add(event_id)
            evidence.append(
                {
                    "ref": event_id,
                    "span": event_id,
                    "provenance": "brain_objects.linked_events",
                }
            )

    return evidence


# ---------------------------------------------------------------------------
# Provenance chain
# ---------------------------------------------------------------------------


def _build_provenance_chain(obj: BrainObject) -> list[dict]:
    """Build provenance chain tracing how this object arrived in the Brain.

    Each entry records a pipeline step, the source or artifact ref, and
    a timestamp where available.
    """
    chain: list[dict] = [
        {"step": "ingested", "source": obj.source, "ts": obj.ingested_ts}
    ]

    if obj.raw_ref:
        chain.append({"step": "stored_raw", "raw_ref": obj.raw_ref})

    if obj.clean_ref:
        chain.append({"step": "cleaned", "clean_ref": obj.clean_ref})

    if obj.summary:
        chain.append({"step": "summarized", "preview": obj.summary[:120]})

    if obj.entities:
        chain.append({"step": "extracted", "entity_count": len(obj.entities)})

    tier_val = obj.tier.value if hasattr(obj.tier, "value") else str(obj.tier)
    chain.append(
        {
            "step": "indexed",
            "tier": tier_val,
            "provenance_hash": obj.provenance_hash,
        }
    )

    return chain


# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------


async def explain(opportunity_id: str) -> dict | None:
    """Build an ExplainTree for an opportunity.

    Steps:
    1. Look up the opportunity by ID (from Postgres brain_objects table).
    2. Find linked events/entities via Kuzu graph (best-effort; stub if unavailable).
    3. Find evidence objects via market_keys and entity mentions.
    4. Build tree with: opportunity summary -> scoring inputs -> evidence -> provenance.
    5. Return tree as nested dict, or None if the opportunity is not found.

    Args:
        opportunity_id: ULID of the BrainObject to explain.

    Returns:
        ExplainTree dict, or None if not found.
    """
    # 1. Look up the opportunity
    obj = await brain_store.get_object(opportunity_id)
    if obj is None:
        logger.warning("explain: opportunity %s not found", opportunity_id)
        return None

    # 2-4. Build tree components
    tree: dict = {
        "opportunity_id": opportunity_id,
        "summary": obj.summary or "",
        "scoring_inputs": _build_scoring_inputs(obj),
        "evidence": await _build_evidence(obj),
        "provenance_chain": _build_provenance_chain(obj),
    }

    return tree
