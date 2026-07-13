"""Pipeline stage 2: clean — extract text from raw content.

Accepts raw bytes from MinIO ``aether-raw`` storage.
Handles text/plain (decoded as UTF-8) and text/html (basic tag stripping).
Stores cleaned text in MinIO ``aether-clean`` bucket.

Returns ``(cleaned_text, clean_ref)`` where ``clean_ref`` is the MinIO object key.
"""

import logging
import re

from server.brain import storage

logger = logging.getLogger(__name__)

_RE_HTML_TAGS = re.compile(r"<[^>]+>")
_RE_HTML_ENTITIES = re.compile(r"&[a-zA-Z]+;|&#\d+;|&#x[0-9a-fA-F]+;")
_RE_WHITESPACE = re.compile(r"\s+")
_RE_PRE_TAG = re.compile(r"<pre[^>]*>(.*?)</pre>", re.DOTALL | re.IGNORECASE)
_RE_SCRIPT_STYLE = re.compile(
    r"<(script|style)[^>]*>[^<]*</\1>", re.DOTALL | re.IGNORECASE
)


def _strip_html(html: str) -> str:
    """Remove HTML tags, scripts, styles, and entities from text.

    Preserves content inside ``<pre>`` blocks verbatim.
    Normalises whitespace (collapses runs into single space).
    """
    # Remove script and style blocks first
    text = _RE_SCRIPT_STYLE.sub("", html)
    # Replace <pre> blocks with preserved content
    text = _RE_PRE_TAG.sub(r"\1", text)
    # Strip HTML tags
    text = _RE_HTML_TAGS.sub("", text)
    # Decode common HTML entities
    text = _RE_HTML_ENTITIES.sub(" ", text)
    # Collapse whitespace
    text = _RE_WHITESPACE.sub(" ", text)
    return text.strip()


def _is_html(data: bytes) -> bool:
    """Heuristic: check if raw content looks like HTML."""
    head = data[:2048].lower()
    markers = [b"<html", b"<!doctype html", b"<!doctype html"]
    return any(marker in head for marker in markers)


async def run(raw_bytes: bytes, source: str) -> tuple[str, str]:
    """Extract and store cleaned text from raw content bytes.

    Args:
        raw_bytes: Raw content bytes (UTF-8 encoded text or HTML).
        source: Source identifier for MinIO key generation.

    Returns:
        Tuple of ``(cleaned_text, clean_minio_key)``.
    """
    # Decode with replacement for non-UTF-8 content
    raw_text = raw_bytes.decode("utf-8", errors="replace")

    # Handle HTML content
    if _is_html(raw_bytes):
        cleaned = _strip_html(raw_text)
        logger.debug(
            "clean: stripped HTML (%d chars -> %d chars)", len(raw_text), len(cleaned)
        )
    else:
        # Plain text: normalise whitespace
        cleaned = _RE_WHITESPACE.sub(" ", raw_text).strip()

    # Store cleaned text in MinIO aether-clean
    cleaned_bytes = cleaned.encode("utf-8")
    _, clean_ref = storage.store_clean(cleaned_bytes, source)

    return cleaned, clean_ref
