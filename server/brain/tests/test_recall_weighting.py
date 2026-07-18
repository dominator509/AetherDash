from datetime import UTC, datetime, timedelta

import pytest

from server.brain.recall import (
    RecallMetadata,
    ScoredRef,
    _apply_decay_and_reliability,
    _decay_weight,
)


def test_staleness_decay_is_monotone_and_matches_half_life() -> None:
    now = datetime(2026, 7, 18, tzinfo=UTC)
    fresh = _decay_weight("news", now, now)
    half_life = _decay_weight("news", now - timedelta(hours=72), now)
    stale = _decay_weight("news", now - timedelta(hours=144), now)

    assert fresh == 1.0
    assert half_life == pytest.approx(0.5)
    assert stale == pytest.approx(0.25)
    assert fresh > half_life > stale
    assert _decay_weight("note", now - timedelta(days=3650), now) == 1.0


def test_reliable_source_ranks_above_equal_unreliable_source() -> None:
    now = datetime(2026, 7, 18, tzinfo=UTC)
    refs = [
        ScoredRef("unreliable", "u", 1.0),
        ScoredRef("reliable", "r", 1.0),
    ]
    metadata = {
        "unreliable": RecallMetadata("news", now, 0.0),
        "reliable": RecallMetadata("news", now, 1.0),
    }

    ranked = _apply_decay_and_reliability(refs, metadata, now=now)

    assert [ref.object_id for ref in ranked] == ["reliable", "unreliable"]
    assert ranked[0].reliability_weight == 1.5
    assert ranked[1].reliability_weight == 0.5


def test_missing_metadata_is_neutral_and_reliability_is_bounded() -> None:
    now = datetime(2026, 7, 18, tzinfo=UTC)
    refs = [
        ScoredRef("missing", "m", 1.0),
        ScoredRef("over", "o", 1.0),
        ScoredRef("under", "u", 1.0),
    ]
    metadata = {
        "over": RecallMetadata("note", now, 9.0),
        "under": RecallMetadata("note", now, -3.0),
    }

    ranked = _apply_decay_and_reliability(refs, metadata, now=now)
    by_id = {ref.object_id: ref for ref in ranked}

    assert by_id["missing"].score == 1.0
    assert by_id["over"].score == 1.5
    assert by_id["under"].score == 0.5
