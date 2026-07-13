"""Link-stage contract tests for EP-201."""

from types import SimpleNamespace

from server.brain.pipeline import link


def test_query_market_texts_returns_payload_market_keys(monkeypatch) -> None:
    points = [
        SimpleNamespace(payload={"market_key": "POLY:one"}),
        SimpleNamespace(payload={"market_key": "POLY:two"}),
    ]
    fake_client = SimpleNamespace(
        get_collections=lambda: SimpleNamespace(
            collections=[SimpleNamespace(name="market_texts")]
        ),
        query_points=lambda **kwargs: SimpleNamespace(points=points)
    )
    monkeypatch.setattr("qdrant_client.QdrantClient", lambda **kwargs: fake_client)

    assert link._query_market_texts("interest rate decision") == [
        "POLY:one",
        "POLY:two",
    ]


def test_ensure_kuzu_schema_propagates_write_failure() -> None:
    class BrokenConnection:
        def execute(self, *_args, **_kwargs):
            raise RuntimeError("graph unavailable")

    try:
        link._ensure_kuzu_schema(BrokenConnection())
    except RuntimeError as exc:
        assert str(exc) == "graph unavailable"
    else:
        raise AssertionError("Kuzu schema failures must park the pipeline")
