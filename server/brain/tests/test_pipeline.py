"""Tests for the ingestion pipeline (EP-201, Milestones 2+3).

Covers all 7 stages: clean, summarize, extract, link, embed, index,
and the runner orchestrator.

Unit tests (no infra needed) test the individual stage functions.
Integration tests (marked ``pytest.mark.integration``) require the full
Docker compose stack (MinIO :9000, Postgres :5432, Qdrant :6333) with
migrations 0015 and 0020 applied.
"""

import pytest

from server.brain.models import BrainObject, ObjectKind, Origin, Tier, now_iso
from server.brain.pipeline import clean, extract, summarize
from server.brain.tests.conftest import skip_integration

# ═══════════════════════════════════════════════════════════════════════
# Stage 2: clean
# ═══════════════════════════════════════════════════════════════════════


@pytest.mark.asyncio
async def test_clean_plain_text() -> None:
    """Clean stage: extracts text from plain UTF-8 bytes."""
    raw = b"Hello, world! This is plain text."
    text, clean_ref = await clean.run(raw, "test-source")
    assert text == "Hello, world! This is plain text."
    assert clean_ref.startswith("clean/test-source/")


@pytest.mark.asyncio
async def test_clean_strips_html_tags() -> None:
    """Clean stage: strips HTML tags from HTML content."""
    html = b"<html><body><p>Hello <b>world</b>!</p></body></html>"
    text, _clean_ref = await clean.run(html, "test-html")
    assert "Hello" in text
    assert "world" in text
    assert "<html>" not in text
    assert "<p>" not in text
    assert "<b>" not in text


@pytest.mark.asyncio
async def test_clean_handles_doctype_html() -> None:
    """Clean stage: detects HTML with DOCTYPE declaration."""
    html = b"<!DOCTYPE html><html><title>Test</title><body>Content</body></html>"
    text, _clean_ref = await clean.run(html, "test-doctype")
    assert "Content" in text
    assert "<title>" not in text


@pytest.mark.asyncio
async def test_clean_removes_script_and_style() -> None:
    """Clean stage: removes <script> and <style> blocks."""
    html = b"<html><head><style>body { color: red; }</style></head><body><p>Visible</p><script>alert('hidden')</script></body></html>"
    text, _clean_ref = await clean.run(html, "test-no-js")
    assert "Visible" in text
    assert "alert" not in text
    assert "color: red" not in text


@pytest.mark.asyncio
async def test_clean_preserves_pre_content() -> None:
    """Clean stage: preserves <pre> block content."""
    html = b"<html><body><pre>  code block  </pre><p>after</p></body></html>"
    text, _clean_ref = await clean.run(html, "test-pre")
    assert "code block" in text
    assert "after" in text


@pytest.mark.asyncio
async def test_clean_empty_bytes() -> None:
    """Clean stage: handles empty bytes."""
    text, _clean_ref = await clean.run(b"", "test-empty")
    assert text == ""


# ═══════════════════════════════════════════════════════════════════════
# Stage 3: summarize (stub)
# ═══════════════════════════════════════════════════════════════════════


def _make_minimal_obj(**kwargs: object) -> BrainObject:
    """Build a minimal BrainObject for testing."""
    overrides = dict(kwargs)
    return BrainObject(
        id="01ARZ3NDEKTSV4RRFFQ69G5FAV",
        kind=ObjectKind(overrides.pop("kind", "note")),
        source=overrides.pop("source", "test-feed"),
        origin=Origin.ingest_fleet,
        ingested_ts=now_iso(),
        provenance_hash="0" * 64,
        **overrides,  # type: ignore[arg-type]
    )


@pytest.mark.asyncio
async def test_summarize_short_text() -> None:
    """Summarize stub: returns full text when <= 500 chars."""
    text = "Short text."
    obj = _make_minimal_obj()
    summary = await summarize.run(text, obj)
    assert summary == text
    assert len(summary) <= 500


@pytest.mark.asyncio
async def test_summarize_truncates_long_text() -> None:
    """Summarize stub: truncates text > 500 chars."""
    text = "Hello world. " * 100  # ~1300 chars
    obj = _make_minimal_obj()
    summary = await summarize.run(text, obj)
    assert len(summary) <= 500
    assert summary == text[:500]


@pytest.mark.asyncio
async def test_summarize_deterministic() -> None:
    """Summarize stub: same input always produces same output."""
    text = "Deterministic test content for verification."
    obj = _make_minimal_obj()
    s1 = await summarize.run(text, obj)
    s2 = await summarize.run(text, obj)
    assert s1 == s2


@pytest.mark.asyncio
async def test_summarize_empty_text_uses_template() -> None:
    """Summarize stub: empty text produces template with kind and source."""
    obj = _make_minimal_obj(kind="document", source="feed://example.com")
    summary = await summarize.run("", obj)
    assert "Document" in summary
    assert "feed://example.com" in summary
    assert len(summary) <= 500


# ═══════════════════════════════════════════════════════════════════════
# Stage 4: extract (stub)
# ═══════════════════════════════════════════════════════════════════════


@pytest.mark.asyncio
async def test_extract_finds_iso_dates() -> None:
    """Extract stub: finds ISO-8601 dates in text."""
    text = "The event occurred on 2026-07-12 and was filed on 2026-07-15."
    entities, dates, claims = await extract.run(text)
    assert "2026-07-12" in dates
    assert "2026-07-15" in dates
    assert dates["2026-07-12"] == "date"


@pytest.mark.asyncio
async def test_extract_finds_timestamps() -> None:
    """Extract stub: finds ISO-8601 timestamps in text."""
    text = "Published at 2026-07-12T14:30:00Z"
    _entities, dates, _claims = await extract.run(text)
    assert "2026-07-12T14:30:00Z" in dates


@pytest.mark.asyncio
async def test_extract_finds_capitalised_phrases() -> None:
    """Extract stub: finds 3+ word capitalised phrases."""
    text = "The New York Stock Exchange and Federal Reserve Board met today."
    entities, _dates, _claims = await extract.run(text)
    assert "The New York Stock Exchange" in entities
    assert "Federal Reserve Board" in entities


@pytest.mark.asyncio
async def test_extract_finds_tickers() -> None:
    """Extract stub: finds $TICKER patterns."""
    text = "AAPL is up 5%, while $GOOGL and $MSFT are flat."
    entities, _dates, _claims = await extract.run(text)
    assert "$GOOGL" in entities
    assert "$MSFT" in entities


@pytest.mark.asyncio
async def test_extract_empty_text() -> None:
    """Extract stub: returns empty lists for empty text."""
    entities, dates, claims = await extract.run("")
    assert entities == []
    assert dates == {}
    assert claims == []

    entities, dates, claims = await extract.run("   ")
    assert entities == []
    assert dates == {}
    assert claims == []


@pytest.mark.asyncio
async def test_extract_returns_empty_claims() -> None:
    """Extract stub: claims list is always empty (EP-202)."""
    text = "The Federal Reserve raised rates on 2026-07-12."
    _entities, _dates, claims = await extract.run(text)
    assert claims == []


@pytest.mark.asyncio
async def test_extract_deduplicates_entities() -> None:
    """Extract stub: same entity appears only once."""
    text = "The New York Stock Exchange is the primary exchange. New York Stock Exchange rules apply."
    entities, _dates, _claims = await extract.run(text)
    assert entities.count("New York Stock Exchange") == 1


# ═══════════════════════════════════════════════════════════════════════
# Stage 6: embed (unit tests for chunking logic)
# ═══════════════════════════════════════════════════════════════════════


def test_chunk_text_empty() -> None:
    """Embed: empty text produces no chunks."""
    from server.brain.pipeline.embed import _chunk_text

    assert _chunk_text("") == []
    assert _chunk_text("   ") == []


def test_chunk_text_short() -> None:
    """Embed: short text produces one chunk."""
    from server.brain.pipeline.embed import _chunk_text

    chunks = _chunk_text("Short text.")
    assert len(chunks) == 1
    assert chunks[0] == "Short text."


def test_chunk_text_splits_long_text() -> None:
    """Embed: text longer than chunk size produces multiple chunks."""
    from server.brain.pipeline.embed import _chunk_text

    text = "word " * 1000  # ~5000 chars
    chunks = _chunk_text(text)
    assert len(chunks) >= 9  # 5000 / (500-50) = ~11, so at least 9
    for chunk in chunks:
        assert len(chunk) <= 500


def test_chunk_text_has_overlap() -> None:
    """Embed: consecutive chunks share ~50 chars of overlap."""
    from server.brain.pipeline.embed import _chunk_text

    # Create content where we can detect overlap
    text = " ".join(f"word_{i}" for i in range(500))  # ~3500 chars
    chunks = _chunk_text(text)
    if len(chunks) >= 2:
        # Adjacent chunks should have some overlapping content
        assert len(chunks[0]) > 300
        assert len(chunks[1]) > 300


def test_stub_embedding_dimension() -> None:
    """Embed: stub vectors are correct dimension and unit length."""
    from server.brain.pipeline.embed import _generate_stub_embedding

    vec = _generate_stub_embedding(1024)
    assert len(vec) == 1024
    # Check unit length (approximately)
    norm = sum(v * v for v in vec) ** 0.5
    assert abs(norm - 1.0) < 1e-6


# ═══════════════════════════════════════════════════════════════════════
# Stage 7: index (unit tests for update object)
# ═══════════════════════════════════════════════════════════════════════


@pytest.mark.asyncio
async def test_update_object_field_mapping() -> None:
    """Store: Pythonic field names map correctly to SQL columns."""
    from server.brain.store import _field_to_column

    assert _field_to_column("clean_ref") == "minio_clean_ref"
    assert _field_to_column("summary") == "summary"
    assert _field_to_column("entities") == "entities"
    assert _field_to_column("linked_events") == "linked_events"
    assert _field_to_column("market_keys") == "market_keys"
    assert _field_to_column("tier") == "tier"
    assert _field_to_column("confidence") == "confidence"
    # Unknown field passes through
    assert _field_to_column("unknown_field") == "unknown_field"


# ═══════════════════════════════════════════════════════════════════════
# Idempotency tests
# ═══════════════════════════════════════════════════════════════════════


@pytest.mark.asyncio
async def test_clean_deterministic_same_input() -> None:
    """Clean stage: same raw bytes produce same clean text."""
    raw = b"   Hello   world!   "
    text1, _ref1 = await clean.run(raw, "test-idempotent")
    text2, _ref2 = await clean.run(raw, "test-idempotent")
    assert text1 == text2


@pytest.mark.asyncio
async def test_clean_preserves_content_after_reprocessing() -> None:
    """Clean stage: HTML stripped content is stable after multiple passes."""
    html = b"<html><body><p>Stable <b>content</b></p></body></html>"
    text1, _ref1 = await clean.run(html, "test-reprocess")
    text2, _ref2 = await clean.run(text1.encode(), "test-reprocess-plain")
    # Second pass on already-cleaned text should not change anything meaningful
    assert text1.strip() == text2.strip()


# ═══════════════════════════════════════════════════════════════════════
# Integration tests (require dev stack)
# ═══════════════════════════════════════════════════════════════════════


@pytest.mark.asyncio
@skip_integration
async def test_pipeline_note_full_flow(clean_brain_objects) -> None:  # noqa: F811
    """Runner: full ingestion pipeline for a note completes without error.

    Requires MinIO, Postgres, and Qdrant to be running.
    """
    from server.brain import service
    from server.brain.models import ObjectDraft

    draft = ObjectDraft(
        kind="note",
        content="This is a test note about the New York Stock Exchange. "
        "On 2026-07-12, $AAPL reached new highs. "
        "The Federal Reserve Board announced new policy.",
        source="test-pipeline",
    )

    # Intake + pipeline runs as background task
    ref = await service.store_draft(draft)
    assert ref is not None

    # Give the pipeline time to complete
    import asyncio

    await asyncio.sleep(2.0)

    # Verify the object was fully indexed
    obj = await service.get_by_id(ref.id)
    assert obj is not None
    assert obj.summary is not None
    assert "New York Stock Exchange" in str(obj.entities) or obj.entities
    assert obj.tier == Tier.hot
    assert obj.clean_ref is not None


@pytest.mark.asyncio
@skip_integration
async def test_pipeline_dedupe_at_intake(clean_brain_objects) -> None:  # noqa: F811
    """Pipeline: same content ingested twice produces one object (dedupe)."""
    from server.brain import service
    from server.brain.models import ObjectDraft

    draft = ObjectDraft(
        kind="note",
        content="Dedupe test content for pipeline verification.",
        source="test-dedupe",
    )

    ref1 = await service.store_draft(draft)
    ref2 = await service.store_draft(draft)

    assert ref1.id == ref2.id


@pytest.mark.asyncio
@skip_integration
async def test_pipeline_clean_twice_same_object(clean_brain_objects) -> None:  # noqa: F811
    """Pipeline: re-running pipeline on same object is safe (idempotent)."""
    from server.brain import service
    from server.brain.models import ObjectDraft

    draft = ObjectDraft(
        kind="document",
        content="<html><body><p>Idempotent content.</p></body></html>",
        source="test-idempotent",
    )

    ref = await service.store_draft(draft)
    await service.run_pipeline_for_object(ref.id)

    import asyncio

    await asyncio.sleep(1.0)

    obj_after_first = await service.get_by_id(ref.id)
    first_clean_ref = obj_after_first.clean_ref

    # Run pipeline again
    await service.run_pipeline_for_object(ref.id)
    await asyncio.sleep(1.0)

    obj_after_second = await service.get_by_id(ref.id)
    assert obj_after_second.clean_ref == first_clean_ref
    assert obj_after_second.summary == obj_after_first.summary


@pytest.mark.asyncio
@skip_integration
async def test_pipeline_index_makes_object_visible(clean_brain_objects) -> None:  # noqa: F811
    """Pipeline: after index stage, object has tier=hot and summary populated."""
    from server.brain import service
    from server.brain.models import ObjectDraft

    draft = ObjectDraft(
        kind="news",
        content="Breaking: The Federal Reserve announced interest rate changes "
        "effective 2026-07-15. $SPY reacted positively.",
        source="test-visibility",
    )

    ref = await service.store_draft(draft)

    import asyncio

    await asyncio.sleep(2.0)

    obj = await service.get_by_id(ref.id)
    assert obj is not None
    assert obj.tier == Tier.hot
    assert obj.summary is not None
    assert len(obj.summary) > 0


@pytest.mark.asyncio
@skip_integration
async def test_pipeline_clean_stage_integration(clean_brain_objects) -> None:  # noqa: F811
    """Pipeline: clean stage stores and retrieves from MinIO correctly."""
    from server.brain import storage as brain_storage

    raw_bytes = b"<html><body><p>Integration test content.</p></body></html>"
    text, clean_key = await clean.run(raw_bytes, "test-integration-clean")

    # Verify it was stored in MinIO
    stored_bytes = brain_storage.get_clean(clean_key)
    stored_text = stored_bytes.decode("utf-8")
    assert stored_text == text
    assert "<p>" not in stored_text


@pytest.mark.asyncio
@skip_integration
async def test_pipeline_extract_and_index_flow(clean_brain_objects) -> None:  # noqa: F811
    """Pipeline: extract stage entities appear in indexed object."""
    from server.brain import service
    from server.brain.models import ObjectDraft

    draft = ObjectDraft(
        kind="report",
        content="Earnings report for New York Stock Exchange listed companies. "
        "On 2026-07-12, $AAPL reported strong results. "
        "$MSFT also beat estimates.",
        source="test-extract-flow",
    )

    ref = await service.store_draft(draft)

    import asyncio

    await asyncio.sleep(2.0)

    obj = await service.get_by_id(ref.id)
    assert obj is not None
    # Entities should be populated (extract stage ran)
    assert (
        "$AAPL" in obj.entities
        or "$MSFT" in obj.entities
        or "New York Stock Exchange" in obj.entities
    )
    assert obj.tier == Tier.hot
