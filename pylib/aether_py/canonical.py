"""Canonical JSON serialization — deterministic byte-identical output."""

import json

from pydantic import BaseModel


def canonical_json_string(obj: object) -> str:
    """Produce canonical JSON string matching Rust's serde_json with preserve_order:
    - Struct field declaration order (Python 3.7+ dict insertion order)
    - No trailing whitespace
    - Decimals as strings (caller's responsibility)
    """
    return json.dumps(obj, ensure_ascii=False, separators=(",", ":"))


def canonical_model_json(model: BaseModel) -> str:
    """Canonical JSON string from a Pydantic model.

    Uses ``model_dump(mode='json', exclude_defaults=True)`` to match Rust's
    ``skip_serializing_if`` behavior (None Option fields, empty Vec/Map fields
    with default, etc.) and serialize enums, Decimal-compatible strings, and
    nested models in their declared field order.

    The output is byte-identical to what Rust's ``serde_json``
    (with ``preserve_order``) produces for the same data.
    """
    obj = model.model_dump(mode="json", exclude_defaults=True)
    return json.dumps(obj, ensure_ascii=False, separators=(",", ":"))
