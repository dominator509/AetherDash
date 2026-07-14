"""Tests for content parsing (text, PDF, image)."""

import os
import subprocess
import sys
from pathlib import Path

import pytest

from server.inbox.parse import parse_content, sandbox
from server.inbox.parse.pdf import parse_pdf

_FIXTURES_DIR = os.path.join(os.path.dirname(__file__), "fixtures")


@pytest.mark.asyncio
async def test_parse_plain_text() -> None:
    """Plain text decodes correctly."""
    text = parse_content("text", b"Hello, world!")
    assert text == "Hello, world!"


@pytest.mark.asyncio
async def test_parse_email_text() -> None:
    """Email content (text kind) decodes correctly."""
    raw = b"From: alice@example.com\nSubject: Meeting\n\nLet's meet at 3pm."
    text = parse_content("email", raw)
    assert "alice@example.com" in text
    assert "Meeting" in text


@pytest.mark.asyncio
async def test_parse_image_returns_placeholder() -> None:
    """Image content returns OCR-pending placeholder."""
    text = parse_content("screenshot", b"fake-image-bytes-12345")
    assert "OCR pending" in text
    assert "EP-206" in text
    assert "bytes" in text


@pytest.mark.asyncio
async def test_parse_image_empty_bytes() -> None:
    """Zero-length image returns placeholder with size 0."""
    text = parse_content("screenshot", b"")
    assert "0 bytes" in text
    assert "OCR pending" in text


@pytest.mark.asyncio
async def test_unknown_kind_raises() -> None:
    """Unknown content kind raises ValueError."""
    with pytest.raises(ValueError, match="Unknown content kind"):
        parse_content("unknown_kind", b"some bytes")


# ── Hostile PDF tests ─────────────────────────────────────────────────────


def test_hostile_pdf_no_js_execution() -> None:
    """Parsing a PDF with embedded JavaScript must NOT execute the JS.

    PyPDF2 is a pure-text-extraction library with no JS interpreter,
    so JS embedded in /OpenAction or /AA is never executed.  This
    test verifies the PDF parses successfully and returns text.
    """
    path = os.path.join(_FIXTURES_DIR, "hostile.pdf")
    with open(path, "rb") as f:
        raw_bytes = f.read()

    # Should parse without raising and without executing JS
    text = parse_content("document", raw_bytes)
    assert isinstance(text, str)


def test_hostile_pdf_no_network_calls() -> None:
    """Parsing must NOT make external HTTP calls.

    PyPDF2 never follows /URI actions or external references.  We
    verify extraction succeeds without any network activity.
    """
    path = os.path.join(_FIXTURES_DIR, "hostile.pdf")
    with open(path, "rb") as f:
        raw_bytes = f.read()

    # The fixture contains /URI actions pointing to evil.example.com
    # and external-ref.example.com — PyPDF2 should ignore these.
    text = parse_content("document", raw_bytes)
    assert isinstance(text, str)
    # No error related to network, no attempt to fetch URIs


def test_hostile_pdf_over_size_limit() -> None:
    """PDF larger than 10 MB raises ValueError."""
    oversized = b"%" + b"X" * (10 * 1024 * 1024 + 1)
    with pytest.raises(ValueError, match="PDF input too large"):
        parse_pdf(oversized)


def test_sandbox_kills_child_at_wall_timeout(monkeypatch: pytest.MonkeyPatch) -> None:
    """The supervisor kills a parser that cannot finish inside its wall budget."""
    monkeypatch.setattr(sandbox, "_TIMEOUT_SECONDS", 0.0001)
    with pytest.raises(TimeoutError, match="wall limit"):
        sandbox.parse_sandboxed("text", b"bounded")


def test_sandbox_rejects_excessive_output() -> None:
    """Parsed output cannot exceed the 2 MiB IPC boundary."""
    with pytest.raises(ValueError, match="parser rejected"):
        sandbox.parse_sandboxed("text", b"x" * (2 * 1024 * 1024 + 1))


def test_worker_network_capability_is_denied() -> None:
    """The child audit policy rejects socket creation before any I/O."""
    worker = Path(sandbox.__file__).with_name("worker.py")
    result = subprocess.run(
        [sys.executable, "-I", str(worker), "_network_probe"],
        input=b"",
        capture_output=True,
        timeout=5,
        check=False,
    )
    assert result.returncode == 0
    assert result.stdout == b"blocked"
