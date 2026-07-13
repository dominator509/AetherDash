"""Pipeline stage 3: summarize — generate a plain-language summary.

DETERMINISTIC STUB for EP-201. Returns the first 500 characters of cleaned
text, or a template ``"Document: {kind} from {source} ({size} bytes)"``.

# TODO(EP-202): replace with real LLM router call
"""

import logging

from server.brain.models import BrainObject

logger = logging.getLogger(__name__)

_MAX_SUMMARY_CHARS = 500


async def run(cleaned_text: str, obj: BrainObject) -> str:
    """Generate a plain-language summary (<= 500 chars).

    DETERMINISTIC STUB:
        First 500 characters of cleaned text. If cleaned text is empty,
        returns a template string.

    Args:
        cleaned_text: Text extracted by the clean stage.
        obj: The associated BrainObject (provides kind, source for template).

    Returns:
        Summary string (<= 500 characters).

    # TODO(EP-202): replace with real LLM router call
    """
    # Strip leading/trailing whitespace for character count
    text = cleaned_text.strip()

    if not text:
        summary = f"Document: {obj.kind.value} from {obj.source} (0 bytes)"
        logger.debug("summarize: empty text, used template")
        return summary[:_MAX_SUMMARY_CHARS]

    # Use first 500 characters — deterministic, no word-boundary logic
    summary = text[:_MAX_SUMMARY_CHARS]

    logger.debug(
        "summarize: stub summary generated (%d chars -> %d chars)",
        len(text),
        len(summary),
    )
    return summary
