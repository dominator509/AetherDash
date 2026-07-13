"""Pipeline stage 5: link — Kuzu graph nodes/edges + Qdrant market linking.

Attempts to create Kuzu Event/Entity/Market/Source nodes and edges.
If Kuzu is unavailable or the database path is unconfigured, logs and
continues (stub behaviour for EP-201).

Queries the ``market_texts`` Qdrant collection for market similarity
matching. If the collection does not exist, returns empty results.

Returns ``(linked_events, market_keys)``.

# TODO(EP-202): replace with real Kuzu + Qdrant linking
"""

import logging
import os
import pathlib

from server.brain.models import BrainObject

logger = logging.getLogger(__name__)

_AETHER_KUZU__PATH = os.environ.get(
    "AETHER_KUZU__PATH", str(pathlib.Path("data") / "kuzu")
)
_AETHER_QDRANT_URL = os.environ.get("AETHER_QDRANT__URL", "http://localhost:6333")

# ── Kuzu graph ──────────────────────────────────────────────────────────

_KUZU_AVAILABLE: bool = False
try:
    import kuzu  # noqa: F401

    _KUZU_AVAILABLE = True
except ImportError:
    logger.debug("link: kuzu-py not available — graph linking stubbed")


def _init_kuzu() -> object | None:
    """Initialise Kuzu database connection.

    Creates the parent directory if it does not exist (Kuzu creates
    the database child directory itself).
    Returns a ``kuzu.Connection`` or ``None`` if Kuzu is not installed.

    Raises:
        Exception: If Kuzu is installed but initialisation fails (fail-closed).
    """
    if not _KUZU_AVAILABLE:
        return None
    # Create parent dir so Kuzu can create its database subdirectory
    kuzu_path = _AETHER_KUZU__PATH
    pathlib.Path(kuzu_path).parent.mkdir(parents=True, exist_ok=True)
    # Fail-closed: if Kuzu is installed but init fails, let the exception
    # propagate so the runner can park the object.
    db = kuzu.Database(kuzu_path)
    conn = kuzu.Connection(db)
    logger.debug("link: Kuzu database opened at %s", kuzu_path)
    return conn


def _ensure_kuzu_schema(conn: object) -> None:
    """Create Kuzu node/edge tables if they do not exist.

    Spec-compliant graph schema per SPEC-002:
    - Event, Entity, Market, Source nodes
    - HAS_TOPIC, MENTIONS, RELATES_TO edges
    """
    conn.execute(
        "CREATE NODE TABLE IF NOT EXISTS Event ("
        "  id STRING, label STRING, ts TIMESTAMP, source STRING, "
        "  PRIMARY KEY (id)"
        ")"
    )
    conn.execute(
        "CREATE NODE TABLE IF NOT EXISTS Entity ("
        "  id STRING, name STRING, kind STRING, "
        "  PRIMARY KEY (id)"
        ")"
    )
    conn.execute(
        "CREATE NODE TABLE IF NOT EXISTS Market ("
        "  id STRING, label STRING, venue STRING, "
        "  PRIMARY KEY (id)"
        ")"
    )
    conn.execute(
        "CREATE NODE TABLE IF NOT EXISTS Source ("
        "  id STRING, uri STRING, "
        "  PRIMARY KEY (id)"
        ")"
    )
    # Edge tables
    conn.execute("CREATE REL TABLE IF NOT EXISTS HAS_TOPIC (  FROM Event TO Entity)")
    conn.execute("CREATE REL TABLE IF NOT EXISTS MENTIONS (  FROM Event TO Entity)")
    conn.execute("CREATE REL TABLE IF NOT EXISTS RELATES_TO (  FROM Event TO Market)")
    logger.debug("link: Kuzu schema ensured")


# ── Qdrant market matching (stub) ───────────────────────────────────────


def _query_market_texts(text: str, limit: int = 5) -> list[str]:
    """Query the ``market_texts`` Qdrant collection for market similarity.

    Returns:
        List of matched market keys ordered by similarity.
    """
    if not text.strip():
        return []

    from qdrant_client import QdrantClient  # noqa: PLC0415

    from server.brain.router_stub import embed_text  # noqa: PLC0415

    client = QdrantClient(url=_AETHER_QDRANT_URL)
    collections = client.get_collections().collections
    if "market_texts" not in {collection.name for collection in collections}:
        # An empty installation has no market corpus to match against yet.
        return []
    result = client.query_points(
        collection_name="market_texts",
        query=embed_text(text),
        limit=limit,
        with_payload=True,
    )
    points = result.points if hasattr(result, "points") else result
    keys: list[str] = []
    for point in points:
        payload = point.payload if hasattr(point, "payload") else {}
        key = payload.get("market_key") or payload.get("id")
        if key and str(key) not in keys:
            keys.append(str(key))
    return keys


# ── Public API ──────────────────────────────────────────────────────────


async def run(
    summary: str,
    entities: list[str],
    obj: BrainObject,
) -> tuple[list[str], list[str]]:
    """Run the link stage: create graph nodes and match markets.

    Args:
        summary: Object summary from summarize stage.
        entities: Extracted entities from extract stage.
        obj: The associated BrainObject.

    Returns:
        Tuple of ``(linked_events, market_keys)``.
    """
    linked_events: list[str] = []
    market_keys: list[str] = []

    # ── Kuzu graph linking ──────────────────────────────────────────
    # If Kuzu is not installed, _init_kuzu returns None and we skip gracefully.
    # If Kuzu IS installed but init fails, the exception propagates so the
    # runner can park the object (fail-closed).
    conn = _init_kuzu()
    if conn is not None:
        _ensure_kuzu_schema(conn)

        # Kuzu uses Cypher for data mutations. Do not swallow write failures:
        # the runner must park the object rather than mark it recallable.
        source_id = f"source_{obj.source}"
        conn.execute(
            "MERGE (s:Source {id: $id}) SET s.uri = $uri",
            {"id": source_id, "uri": obj.source},
        )

        # Create Event node
        event_id = f"event_{obj.id}"
        conn.execute(
            "MERGE (e:Event {id: $id}) "
            "SET e.label = $label, e.ts = timestamp($ts), e.source = $source",
            {
                "id": event_id,
                "label": summary or obj.kind.value,
                "ts": obj.ingested_ts,
                "source": source_id,
            },
        )
        linked_events.append(event_id)

        # Create Entity nodes and MENTIONS edges
        for entity_name in entities[:20]:  # limit to 20 entities
            entity_id = f"entity_{obj.id}_{entity_name[:32]}"
            conn.execute(
                "MERGE (n:Entity {id: $id}) SET n.name = $name, n.kind = $kind",
                {"id": entity_id, "name": entity_name, "kind": "extracted"},
            )
            conn.execute(
                "MATCH (e:Event {id: $event_id}), (n:Entity {id: $entity_id}) "
                "MERGE (e)-[:MENTIONS]->(n)",
                {"event_id": event_id, "entity_id": entity_id},
            )

        logger.debug(
            "link: Kuzu graph updated — event=%s, entities=%d",
            event_id,
            min(len(entities), 20),
        )
    else:
        logger.info("link: Kuzu not available — graph linking stubbed")

    # ── Qdrant market matching ──────────────────────────────────────
    market_keys = _query_market_texts(summary or "")
    if market_keys:
        logger.debug("link: matched %d markets via Qdrant", len(market_keys))
        if conn is not None:
            for market_key in market_keys:
                conn.execute(
                    "MERGE (m:Market {id: $id}) SET m.label = $label",
                    {"id": market_key, "label": market_key},
                )
                conn.execute(
                    "MATCH (e:Event {id: $event_id}), (m:Market {id: $market_id}) "
                    "MERGE (e)-[:RELATES_TO]->(m)",
                    {"event_id": event_id, "market_id": market_key},
                )

    return linked_events, market_keys
