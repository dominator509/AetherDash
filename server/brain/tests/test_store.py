"""Integration tests for Brain.Store and Brain.Get.

Requires the Docker compose stack (MinIO on :9000, Postgres on :5432, ClickHouse on :8123)
with migrations 0015 and 0020 applied.
"""

import pytest

from server.brain import service, storage, store
from server.brain.models import (
    BrainRef,
    ObjectDraft,
    ObjectKind,
    Origin,
    compute_provenance_hash,
)
from server.brain.tests.conftest import skip_integration

pytestmark = [
    pytest.mark.integration,
    pytest.mark.asyncio,
]


# ── Tests ────────────────────────────────────────────────────────────────


@skip_integration
async def test_store_note_returns_valid_brain_ref(clean_brain_objects) -> None:
    """Store a note → returns BrainRef with valid ULID and provenance hash."""
    draft = ObjectDraft(
        kind="note",
        content="This is a test note about market conditions.",
        source="test-feed",
    )

    ref = await service.store_draft(draft)

    assert ref is not None
    assert len(ref.id) == 26, f"Expected 26-char ULID, got {len(ref.id)}: {ref.id}"
    assert len(ref.provenance_hash) == 64, "Provenance hash must be 64 hex chars"
    assert all(c in "0123456789abcdef" for c in ref.provenance_hash)


@skip_integration
async def test_store_same_content_twice_returns_same_ref(clean_brain_objects) -> None:
    """Store same content twice → returns same BrainRef (dedupe)."""
    draft = ObjectDraft(
        kind="note",
        content="Dedupe test content — identical string.",
        source="test-feed",
    )

    ref1 = await service.store_draft(draft)
    ref2 = await service.store_draft(draft)

    assert ref1.id == ref2.id
    assert ref1.provenance_hash == ref2.provenance_hash


@skip_integration
async def test_get_by_brain_ref_returns_correct_fields(clean_brain_objects) -> None:
    """Get by BrainRef → returns object with correct fields."""
    draft = ObjectDraft(
        kind="document",
        content="Quarterly earnings report content for Q2 2026.",
        source="feed://sec.gov",
    )

    ref = await service.store_draft(draft)

    obj = await service.get(ref)
    assert obj is not None
    assert obj.id == ref.id
    assert obj.kind == ObjectKind.document
    assert obj.source == "feed://sec.gov"
    assert obj.origin == Origin.ingest_fleet
    assert obj.provenance_hash == ref.provenance_hash
    assert obj.raw_ref is not None
    assert obj.raw_ref.startswith("raw/feed/sec.gov/")


@skip_integration
async def test_get_nonexistent_returns_none(clean_brain_objects) -> None:
    """Get by nonexistent BrainRef → returns None."""
    ref = BrainRef(
        id="01ARZ3NDEKTSV4RRFFQ69G5FAV",
        provenance_hash="0" * 64,
    )
    obj = await service.get(ref)
    assert obj is None


@skip_integration
async def test_provenance_hash_deterministic_across_calls(clean_brain_objects) -> None:
    """Same inputs produce identical provenance hash."""
    content = b"deterministic content"
    source = "test-feed"
    raw_sha256, _ = storage.store_raw(content, source)
    ingested_ts = "2026-07-12T00:00:00.000Z"

    h1 = compute_provenance_hash(source, raw_sha256, ingested_ts)
    h2 = compute_provenance_hash(source, raw_sha256, ingested_ts)

    assert h1 == h2


@skip_integration
async def test_ingest_event_emitted(clean_brain_objects) -> None:
    """Every stored object gets one durable event with its exact source rung."""
    draft = ObjectDraft(
        kind="email",
        content="Test email content for ingest event verification.",
        source="inbox://user@example.com",
    )

    ref = await service.store_draft(draft, ladder_rung=3)
    assert ref is not None
    pool = await store.get_pool()
    row = await pool.fetchrow(
        """
        SELECT source, ladder_rung, bytes, status, trace_id
        FROM ingest_source_events WHERE object_id=$1
        """,
        ref.id,
    )
    assert row is not None
    assert row["source"] == "inbox://user@example.com"
    assert row["ladder_rung"] == 3
    assert row["bytes"] == len(draft.content.encode())
    assert row["status"] == "ingested"
    assert len(row["trace_id"]) == 26

    count = await pool.fetchval(
        "SELECT count(*) FROM ingest_source_events WHERE object_id=$1", ref.id
    )
    assert count == 1
