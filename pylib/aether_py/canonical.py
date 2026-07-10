"""Canonical JSON serialization — deterministic byte-identical output."""

import json


def canonical_json_string(obj: object) -> str:
    """Produce canonical JSON string matching Rust's serde_json with preserve_order:
    - Struct field declaration order (Python 3.7+ dict insertion order)
    - No trailing whitespace
    - Decimals as strings (caller's responsibility)
    """
    return json.dumps(obj, ensure_ascii=True, separators=(",", ":"))
