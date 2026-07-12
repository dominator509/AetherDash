"""Typed golden round-trip tests — construct Pydantic models, hash canonical dump."""

import hashlib
import json
import os

import pytest
from aether_py.canonical import canonical_model_json
from aether_py.models import TYPE_REGISTRY

GD = os.path.join(
    os.path.dirname(__file__), "..", "..", "..", "testdata", "golden", "core"
)
ALL = [
    "money",
    "market_key",
    "confidence",
    "edge",
    "quote",
    "order_book",
    "order_intent",
    "risk_verdict",
    "order",
    "fill",
    "position",
    "caps_snapshot",
    "market",
    "price_semantics",
    "opportunity",
    "audit_event",
    "error_envelope",
    # P1-7: Adversarial canonical vectors (cross-language)
    "unicode",
    "ordering",
    "null_omission",
    "empty_collections",
]


def sha256(s: str) -> str:
    return hashlib.sha256(s.encode()).hexdigest()


def load(fn: str) -> list[dict[str, object]]:
    with open(os.path.join(GD, f"{fn}.json"), encoding="utf-8") as f:
        return json.load(f)


@pytest.mark.parametrize("fn", ALL)
def test_typed_golden_round_trips(fn: str) -> None:
    entries = load(fn)
    assert len(entries) > 0
    for e in entries:
        typ = str(e["type"])
        val = e["value"]

        model_cls = TYPE_REGISTRY.get(typ)
        if model_cls is None:
            raise AssertionError(f"Unknown type label: {typ}")

        # Construct the model — validation happens here
        model = model_cls.model_validate(val)

        # Hash the canonical model dump (not the raw input dict)
        canonical = canonical_model_json(model)
        assert sha256(canonical) == str(e["sha256"]), f"{e['name']}: SHA-256 mismatch"
