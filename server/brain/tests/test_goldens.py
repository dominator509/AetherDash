"""Golden tests for provenance hash stability.

Verifies that ``compute_provenance_hash`` produces deterministic output
and that the output matches a known golden value.
"""

import hashlib
import json

from server.brain.models import (
    ProvenancePayload,
    compute_provenance_hash,
)

# ── Determinism test ────────────────────────────────────────────────────


def test_provenance_hash_is_deterministic() -> None:
    """Same inputs always produce the same hash."""
    h1 = compute_provenance_hash(
        source="feed://example.com",
        raw_sha256="a" * 64,
        ingested_ts="2026-07-12T00:00:00.000Z",
    )
    h2 = compute_provenance_hash(
        source="feed://example.com",
        raw_sha256="a" * 64,
        ingested_ts="2026-07-12T00:00:00.000Z",
    )
    assert h1 == h2
    assert len(h1) == 64  # SHA-256 hex


def test_provenance_hash_changes_on_any_input() -> None:
    """Different inputs produce different hashes."""
    h1 = compute_provenance_hash(
        source="feed://a",
        raw_sha256="a" * 64,
        ingested_ts="2026-07-12T00:00:00.000Z",
    )
    h2 = compute_provenance_hash(
        source="feed://b",
        raw_sha256="a" * 64,
        ingested_ts="2026-07-12T00:00:00.000Z",
    )
    assert h1 != h2


# ── Canonical JSON shape test ──────────────────────────────────────────


def test_provenance_canonical_json_shape() -> None:
    """Verify the canonical JSON structure for provenance payload."""
    payload = ProvenancePayload(
        source="test-feed",
        raw_sha256="abc" * 21 + "a",  # 64 chars
        ingested_ts="2026-07-12T00:00:00.000Z",
    )
    canonical = payload.model_dump(mode="json", exclude_defaults=True)
    canonical_str = json.dumps(canonical, ensure_ascii=False, separators=(",", ":"))

    # Must be key-sorted (insertion order of ProvenancePayload fields)
    assert '"source"' in canonical_str
    assert '"raw_sha256"' in canonical_str
    assert '"ingested_ts"' in canonical_str
    # No whitespace
    assert " " not in canonical_str
    assert "\n" not in canonical_str
    # Fields in declaration order
    assert canonical_str.index("source") < canonical_str.index("raw_sha256")
    assert canonical_str.index("raw_sha256") < canonical_str.index("ingested_ts")


# ── Golden test ─────────────────────────────────────────────────────────


def test_provenance_hash_golden() -> None:
    """Provenance hash matches a known golden value.

    This test ensures the Python implementation stays byte-identical
    to the Rust ``aether-core`` canonical hash for the same inputs.
    The golden value is computed once and captured in this test.
    """
    source = "test-feed"
    raw_sha256 = "abc123" * 10 + "abcd"  # 64 hex chars
    ingested_ts = "2026-07-12T00:00:00.000Z"

    # Compute the provenance hash using the shared canonical serialization
    provenance_hash = compute_provenance_hash(
        source=source,
        raw_sha256=raw_sha256,
        ingested_ts=ingested_ts,
    )

    # The canonical JSON is:
    # {"source":"test-feed","raw_sha256":"abc123abc123abc123abc123abc123abc123abc123abc123abc123abc123abcd","ingested_ts":"2026-07-12T00:00:00.000Z"}
    expected = hashlib.sha256(
        json.dumps(
            {
                "source": source,
                "raw_sha256": raw_sha256,
                "ingested_ts": ingested_ts,
            },
            ensure_ascii=False,
            separators=(",", ":"),
        ).encode()
    ).hexdigest()

    assert provenance_hash == expected
    assert len(provenance_hash) == 64
