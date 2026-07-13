"""Unit tests for the vault generator (Milestone 6).

Uses mocks for the Postgres store layer and patches ``_VAULT_DIR``
so tests use a temporary directory instead of the real ``vault/``.
"""

import tempfile
from pathlib import Path
from unittest.mock import AsyncMock, patch

import pytest

from server.brain.models import (
    BrainObject,
    ObjectKind,
    Origin,
    Tier,
    TrustLevel,
    now_iso,
)
from server.brain.vault import regenerate_vault


def _make_brain_object(**overrides: object) -> BrainObject:
    """Build a BrainObject with sensible defaults for vault testing."""
    defaults: dict = {
        "id": "01ARZ3NDEKTSV4RRFFQ69G5FAV",
        "kind": ObjectKind.news,
        "source": "feed://news-api",
        "origin": Origin.ingest_fleet,
        "trust": TrustLevel.medium,
        "ingested_ts": now_iso(),
        "provenance_hash": "a" * 64,
        "tier": Tier.hot,
        "entities": ["Federal Reserve", "$SPY"],
        "linked_events": ["event_rate_hike_2026"],
        "market_keys": ["SPY-options"],
        "summary": "Federal Reserve raises interest rates by 25bps.",
    }
    defaults.update(overrides)
    return BrainObject(**defaults)


# ── Tests ─────────────────────────────────────────────────────────────────


@pytest.mark.asyncio
@patch("server.brain.store.list_objects")
async def test_vault_regenerates_without_error(mock_list: AsyncMock) -> None:
    """Vault regenerates without raising an exception."""
    mock_list.return_value = []
    with tempfile.TemporaryDirectory() as tmpdir:
        with patch("server.brain.vault._VAULT_DIR", tmpdir):
            await regenerate_vault()


@pytest.mark.asyncio
@patch("server.brain.store.list_objects")
async def test_vault_creates_folders_by_kind(mock_list: AsyncMock) -> None:
    """Vault creates subdirectories grouped by object kind then market."""
    news_obj = _make_brain_object(
        id="01ARZ3NDEKTSV4RRFFQ69G5FA1",
        kind=ObjectKind.news,
        market_keys=["SPY-options"],
        summary="News item 1",
    )
    report_obj = _make_brain_object(
        id="01ARZ3NDEKTSV4RRFFQ69G5FA2",
        kind=ObjectKind.report,
        market_keys=["AAPL-earnings"],
        summary="Report item 1",
    )
    note_obj = _make_brain_object(
        id="01ARZ3NDEKTSV4RRFFQ69G5FA3",
        kind=ObjectKind.note,
        market_keys=[],
        summary="Note item 1",
    )
    mock_list.return_value = [news_obj, report_obj, note_obj]

    with tempfile.TemporaryDirectory() as tmpdir:
        with patch("server.brain.vault._VAULT_DIR", tmpdir):
            await regenerate_vault()
            vault = Path(tmpdir)
            assert (vault / "news" / "SPY-options").is_dir()
            assert (vault / "report" / "AAPL-earnings").is_dir()
            assert (vault / "note" / "_no_market").is_dir()


@pytest.mark.asyncio
@patch("server.brain.store.list_objects")
async def test_vault_creates_md_files_with_frontmatter(mock_list: AsyncMock) -> None:
    """Vault creates .md files containing YAML frontmatter."""
    obj = _make_brain_object(
        summary="Test frontmatter content",
        market_keys=["SPY-options"],
    )
    mock_list.return_value = [obj]

    with tempfile.TemporaryDirectory() as tmpdir:
        with patch("server.brain.vault._VAULT_DIR", tmpdir):
            await regenerate_vault()
            md_files = list(Path(tmpdir).rglob("*.md"))
            assert len(md_files) >= 1

            content = md_files[0].read_text(encoding="utf-8")
            assert content.startswith("---")
            assert "id:" in content
            assert "kind:" in content
            assert "provenance_hash:" in content
            assert "source:" in content
            assert "trust:" in content
            assert "ingested_ts:" in content


@pytest.mark.asyncio
@patch("server.brain.store.list_objects")
async def test_vault_uses_object_id_filename(mock_list: AsyncMock) -> None:
    """Vault filenames are ``{object_id}.md`` for uniqueness."""
    obj = _make_brain_object(
        id="01ARZ3NDEKTSV4RRFFQ69G5FAV",
        market_keys=["SPY-options"],
        summary="Test unique filename",
    )
    mock_list.return_value = [obj]

    with tempfile.TemporaryDirectory() as tmpdir:
        with patch("server.brain.vault._VAULT_DIR", tmpdir):
            await regenerate_vault()
            expected_file = (
                Path(tmpdir) / "news" / "SPY-options" / "01ARZ3NDEKTSV4RRFFQ69G5FAV.md"
            )
            assert expected_file.exists()


@pytest.mark.asyncio
@patch("server.brain.store.list_objects")
async def test_vault_includes_wikilinks(mock_list: AsyncMock) -> None:
    """Vault .md files include wikilinks for linked_events."""
    obj = _make_brain_object(
        linked_events=["event_rate_hike"],
        market_keys=["SPY-options"],
        summary="Test wikilinks",
    )
    mock_list.return_value = [obj]

    with tempfile.TemporaryDirectory() as tmpdir:
        with patch("server.brain.vault._VAULT_DIR", tmpdir):
            await regenerate_vault()
            md_files = list(Path(tmpdir).rglob("*.md"))
            content = md_files[0].read_text(encoding="utf-8")
            assert "[[event_rate_hike]]" in content


@pytest.mark.asyncio
@patch("server.brain.store.list_objects")
async def test_vault_excludes_raw_email_bodies(mock_list: AsyncMock) -> None:
    """Vault excludes raw email bodies beyond the summary.

    Email objects should only have their summary included, not the full
    raw body section.
    """
    email_obj = _make_brain_object(
        id="01ARZ3NDEKTSV4RRFFQ69G5FAE",
        kind=ObjectKind.email,
        market_keys=["SPY-options"],
        summary="Email summary only",
    )
    mock_list.return_value = [email_obj]

    with tempfile.TemporaryDirectory() as tmpdir:
        with patch("server.brain.vault._VAULT_DIR", tmpdir):
            await regenerate_vault()
            vault = Path(tmpdir)
            email_dir = vault / "email" / "SPY-options"
            assert email_dir.is_dir()
            md_files = list(email_dir.rglob("*.md"))
            assert len(md_files) >= 1
            content = md_files[0].read_text(encoding="utf-8")
            # Should have the summary
            assert "Email summary only" in content
            # Should NOT include raw body section for email objects
            assert "## Raw Body" not in content


@pytest.mark.asyncio
@patch("server.brain.store.list_objects")
async def test_vault_excludes_low_trust_inbox_beyond_summary(
    mock_list: AsyncMock,
) -> None:
    """Vault excludes low-trust inbox items that lack a summary."""
    low_trust_summarized = _make_brain_object(
        id="01ARZ3NDEKTSV4RRFFQ69G5FAL",
        kind=ObjectKind.email,
        source="inbox://user@example.com",
        origin=Origin.inbox,
        trust=TrustLevel.low,
        market_keys=["SPY-options"],
        summary="Low-trust email that was summarised",
    )
    low_trust_no_summary = _make_brain_object(
        id="01ARZ3NDEKTSV4RRFFQ69G5FAN",
        kind=ObjectKind.email,
        source="inbox://user@example.com",
        origin=Origin.inbox,
        trust=TrustLevel.low,
        market_keys=[],
        summary=None,
    )
    mock_list.return_value = [low_trust_summarized, low_trust_no_summary]

    with tempfile.TemporaryDirectory() as tmpdir:
        with patch("server.brain.vault._VAULT_DIR", tmpdir):
            await regenerate_vault()
            md_files = list(Path(tmpdir).rglob("*.md"))
            # Only the summarized one should appear
            content = "".join(f.read_text(encoding="utf-8") for f in md_files)
            assert "Low-trust email that was summarised" in content
            assert "01ARZ3NDEKTSV4RRFFQ69G5FAN" not in content


@pytest.mark.asyncio
@patch("server.brain.store.list_objects")
async def test_vault_writes_gitkeep_and_gitignore(mock_list: AsyncMock) -> None:
    """Vault root contains .gitkeep and .gitignore."""
    mock_list.return_value = []

    with tempfile.TemporaryDirectory() as tmpdir:
        with patch("server.brain.vault._VAULT_DIR", tmpdir):
            await regenerate_vault()
            assert Path(tmpdir, ".gitkeep").exists()
            assert Path(tmpdir, ".gitignore").exists()
            gitignore_content = Path(tmpdir, ".gitignore").read_text(encoding="utf-8")
            assert gitignore_content == "*\n"


@pytest.mark.asyncio
@patch("server.brain.store.list_objects")
async def test_vault_includes_market_keys(mock_list: AsyncMock) -> None:
    """Vault includes market keys as wikilinks when present."""
    obj = _make_brain_object(
        market_keys=["SPY-240712", "AAPL-earnings"],
        summary="Test market keys",
    )
    mock_list.return_value = [obj]

    with tempfile.TemporaryDirectory() as tmpdir:
        with patch("server.brain.vault._VAULT_DIR", tmpdir):
            await regenerate_vault()
            md_files = list(Path(tmpdir).rglob("*.md"))
            content = md_files[0].read_text(encoding="utf-8")
            assert "[[SPY-240712]]" in content
            assert "[[AAPL-earnings]]" in content
