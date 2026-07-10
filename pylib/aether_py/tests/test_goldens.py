"""Typed golden round-trip tests — construct Pydantic models to validate, hash original JSON."""

import hashlib
import json
import os
from decimal import Decimal

import pytest
from aether_py.canonical import canonical_json_string
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
]


def sha256(s: str) -> str:
    return hashlib.sha256(s.encode()).hexdigest()


def load(fn: str) -> list[dict[str, object]]:
    with open(os.path.join(GD, f"{fn}.json")) as f:
        return json.load(f)


@pytest.mark.parametrize("fn", ALL)
def test_typed_golden_round_trips(fn: str) -> None:
    entries = load(fn)
    assert len(entries) > 0
    for e in entries:
        typ = str(e["type"])
        val = e["value"]
        # --- Validate by constructing the actual type ---
        if typ == "MarketKey":
            s = str(val)
            assert s.startswith("mkt:") and s.count(":") >= 2, f"MarketKey invalid: {s}"
        elif typ == "Confidence":
            s = str(val)
            d = Decimal(s)
            assert d >= Decimal("0") and d <= Decimal("1"), (
                f"Confidence out of range: {s}"
            )
        elif typ in TYPE_REGISTRY:
            TYPE_REGISTRY[typ].model_validate(val)
        else:
            raise AssertionError(f"Unknown type: {typ}")
        # --- Hash the original JSON (Rust's canonical output) ---
        canonical = canonical_json_string(val)
        assert sha256(canonical) == str(e["sha256"]), f"{e['name']}: SHA-256 mismatch"
