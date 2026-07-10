"""Test Python types against Rust-generated golden vectors with SHA-256 verification."""

import hashlib
import json
import os

import pytest
from aether_py.canonical import canonical_json_string

GOLDEN_DIR = os.path.join(
    os.path.dirname(__file__), "..", "..", "..", "testdata", "golden", "core"
)


def sha256(data: str) -> str:
    return hashlib.sha256(data.encode()).hexdigest()


def load_goldens(filename: str) -> list[dict[str, object]]:
    path = os.path.join(GOLDEN_DIR, filename)
    with open(path) as f:
        return json.load(f)  # type: ignore[no-any-return]


@pytest.mark.parametrize("golden_file", ["money.json", "edge.json"])
def test_golden_vectors_match_sha256(golden_file: str) -> None:
    entries = load_goldens(golden_file)
    assert len(entries) > 0, f"No golden entries in {golden_file}"
    for entry in entries:
        canonical = canonical_json_string(entry["value"])
        computed_hash = sha256(canonical)
        assert computed_hash == entry["sha256"], (
            f"{entry['name']}: SHA-256 mismatch\n"
            f"  canonical: {canonical}\n"
            f"  expected:  {entry['sha256']}\n"
            f"  computed:  {computed_hash}"
        )
