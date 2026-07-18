"""Deterministic one-hop Kuzu expansion for recall seed objects."""

from dataclasses import dataclass
from typing import Any


@dataclass(frozen=True)
class GraphCandidate:
    object_id: str
    shared_edges: int


def _collect_rows(result: Any) -> list[list[Any]]:
    rows: list[list[Any]] = []
    while result.has_next():
        rows.append(result.get_next())
    return rows


def expand_connection(
    connection: Any, seed_object_ids: list[str], *, limit: int = 50
) -> list[GraphCandidate]:
    if limit < 1 or not seed_object_ids:
        return []
    seed_ids = [f"event_{object_id}" for object_id in dict.fromkeys(seed_object_ids)]
    counts: dict[str, int] = {}
    queries = (
        "MATCH (seed:Event)-[:MENTIONS]->(:Entity)<-[:MENTIONS]-(neighbor:Event) "
        "WHERE seed.id IN $seed_ids AND NOT neighbor.id IN $seed_ids "
        "RETURN neighbor.id, count(*) AS shared_edges "
        "ORDER BY shared_edges DESC LIMIT $query_limit",
        "MATCH (seed:Event)-[:RELATES_TO]->(:Market)<-[:RELATES_TO]-(neighbor:Event) "
        "WHERE seed.id IN $seed_ids AND NOT neighbor.id IN $seed_ids "
        "RETURN neighbor.id, count(*) AS shared_edges "
        "ORDER BY shared_edges DESC LIMIT $query_limit",
    )
    for query in queries:
        result = connection.execute(query, {"seed_ids": seed_ids, "query_limit": limit})
        for event_id, shared_edges in _collect_rows(result):
            object_id = str(event_id).removeprefix("event_")
            counts[object_id] = counts.get(object_id, 0) + int(shared_edges)
    ranked = sorted(counts.items(), key=lambda item: (-item[1], item[0]))[:limit]
    return [GraphCandidate(object_id=oid, shared_edges=count) for oid, count in ranked]


def source_reliabilities(connection: Any, sources: list[str]) -> dict[str, float]:
    source_ids = [f"source_{source}" for source in dict.fromkeys(sources)]
    result = connection.execute(
        "MATCH (source:Source) WHERE source.id IN $source_ids "
        "RETURN source.id, source.reliability",
        {"source_ids": source_ids},
    )
    values: dict[str, float] = {}
    for source_id, reliability in _collect_rows(result):
        if reliability is not None:
            values[str(source_id).removeprefix("source_")] = min(
                1.0, max(0.0, float(reliability))
            )
    return values
