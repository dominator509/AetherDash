"""Tests for the filing module (submitting parsed content to Brain.Store)."""

from unittest.mock import AsyncMock, patch

import pytest

from server.inbox.filing import file_to_brain


@pytest.mark.asyncio
async def test_file_to_brain_returns_object_id() -> None:
    """file_to_brain returns the brain object ID."""
    mock_ref = AsyncMock()
    mock_ref.id = "01ARZ3NDEKTSV4RRFFQ69G5FAV"

    with patch("server.brain.service.store_draft", return_value=mock_ref):
        obj_id = await file_to_brain(
            kind="email",
            source="alice@example.com",
            raw_bytes=b"raw content",
            cleaned_text="cleaned content",
        )

    assert obj_id == "01ARZ3NDEKTSV4RRFFQ69G5FAV"


@pytest.mark.asyncio
async def test_file_to_brain_sets_origin_inbox() -> None:
    """file_to_brain calls store_draft with origin='inbox'."""
    mock_ref = AsyncMock()
    mock_ref.id = "01ARZ3NDEKTSV4RRFFQQQQQQQQ"

    with patch("server.brain.service.store_draft") as mock_store:
        mock_store.return_value = mock_ref
        await file_to_brain(
            kind="document",
            source="sender@example.com",
            raw_bytes=b"pdf bytes",
            cleaned_text="extracted pdf text",
        )

    mock_store.assert_called_once()
    _call_kwargs = mock_store.call_args
    # Check that origin and trust were passed
    assert _call_kwargs[1].get("origin") == "inbox"
    assert _call_kwargs[1].get("trust") == "low"
    assert _call_kwargs[1].get("raw_content") == b"pdf bytes"


@pytest.mark.asyncio
async def test_file_to_brain_provenance_fields() -> None:
    """file_to_brain passes correct ObjectDraft fields to store_draft."""
    mock_ref = AsyncMock()
    mock_ref.id = "01ARZ3NDEKTSV4RRFFQ69G5FAV"

    with patch("server.brain.service.store_draft") as mock_store:
        mock_store.return_value = mock_ref
        await file_to_brain(
            kind="screenshot",
            source="camera@device.local",
            raw_bytes=b"\x89PNG\r\n\x1a\n",
            cleaned_text="[Image: 8 bytes, OCR pending -- EP-206]",
        )

    mock_store.assert_called_once()
    _call_args = mock_store.call_args
    draft = _call_args[0][0]  # First positional arg is the ObjectDraft

    assert draft.kind == "screenshot"
    assert draft.source == "camera@device.local"
    assert draft.content == "[Image: 8 bytes, OCR pending -- EP-206]"


@pytest.mark.asyncio
async def test_file_to_brain_email_kind() -> None:
    """file_to_brain handles 'email' kind correctly."""
    mock_ref = AsyncMock()
    mock_ref.id = "01ARZ3NDEKTSV4RRFFQ69G5FAV"

    with patch("server.brain.service.store_draft") as mock_store:
        mock_store.return_value = mock_ref
        obj_id = await file_to_brain(
            kind="email",
            source="alice@example.com",
            raw_bytes=b"email raw",
            cleaned_text="email text",
        )

    assert obj_id == "01ARZ3NDEKTSV4RRFFQ69G5FAV"
    mock_store.assert_called_once()
    draft = mock_store.call_args[0][0]
    assert draft.kind == "email"
    assert draft.source == "alice@example.com"
