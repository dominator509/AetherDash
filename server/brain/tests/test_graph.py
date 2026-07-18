from pathlib import Path

import kuzu

from server.brain.graph import expand_connection, source_reliabilities
from server.brain.pipeline.link import _ensure_kuzu_schema
from server.brain.recall import _rrf_fuse_with_graph


def test_graph_fusion_surfaces_neighbor_and_preserves_seed_rank() -> None:
    qdrant = [{"object_id": "seed", "score": 1.0}]
    fts = [{"object_id": "seed", "provenance_hash": "hash", "score": 1.0}]
    graph = [{"object_id": "neighbor", "shared_edges": 2}]

    ranked = _rrf_fuse_with_graph(qdrant, fts, graph, 2)

    assert [ref.object_id for ref in ranked] == ["seed", "neighbor"]
    assert ranked[1].graph_rank == 1


def test_one_hop_expansion_finds_shared_entity_and_market_neighbors(
    tmp_path: Path,
) -> None:
    database = kuzu.Database(str(tmp_path / "graph.kuzu"))
    connection = kuzu.Connection(database)
    _ensure_kuzu_schema(connection)
    for event_id in ("event_seed", "event_entity_neighbor", "event_market_neighbor"):
        connection.execute(
            "CREATE (:Event {id: $id, label: $id, source: 'source_fixture'})",
            {"id": event_id},
        )
    connection.execute("CREATE (:Entity {id: 'entity_shared', name: 'Shared'})")
    connection.execute("CREATE (:Market {id: 'market_shared', label: 'Shared'})")
    connection.execute(
        "MATCH (e:Event), (n:Entity {id: 'entity_shared'}) "
        "WHERE e.id IN ['event_seed','event_entity_neighbor'] "
        "CREATE (e)-[:MENTIONS]->(n)"
    )
    connection.execute(
        "MATCH (e:Event), (m:Market {id: 'market_shared'}) "
        "WHERE e.id IN ['event_seed','event_market_neighbor'] "
        "CREATE (e)-[:RELATES_TO]->(m)"
    )

    candidates = expand_connection(connection, ["seed"], limit=10)

    assert {candidate.object_id for candidate in candidates} == {
        "entity_neighbor",
        "market_neighbor",
    }
    assert all(candidate.shared_edges == 1 for candidate in candidates)
    assert expand_connection(connection, ["seed"], limit=1)[0].object_id == (
        "entity_neighbor"
    )


def test_source_reliability_is_read_and_bounded(tmp_path: Path) -> None:
    database = kuzu.Database(str(tmp_path / "reliability.kuzu"))
    connection = kuzu.Connection(database)
    _ensure_kuzu_schema(connection)
    connection.execute(
        "CREATE (:Source {id: 'source_fixture', uri: 'fixture', reliability: 0.8})"
    )

    assert source_reliabilities(connection, ["fixture", "missing"]) == {"fixture": 0.8}
