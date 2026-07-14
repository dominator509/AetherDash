"""Content parsers — plain text, PDF (safe), image (OCR placeholder).

Exports ``parse_content(kind, raw_bytes) -> str`` which dispatches to the
appropriate parser based on content kind.
"""

import logging

from server.inbox.parse.sandbox import parse_sandboxed

logger = logging.getLogger(__name__)


def parse_content(kind: str, raw_bytes: bytes) -> str:
    """Parse raw content bytes into cleaned text.

    Args:
        kind: Content kind — ``"text"``, ``"email"``, ``"document"``,
              or ``"screenshot"``.
        raw_bytes: Raw content bytes to parse.

    Returns:
        Cleaned text string extracted from the content.

    Raises:
        ValueError: For unknown content kinds.
    """
    return parse_sandboxed(kind, raw_bytes)
