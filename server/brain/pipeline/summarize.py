"""Pipeline stage 3: summarize — generate a plain-language summary.

Calls the LLM Router (``server.llm_router.client.complete``) with
purpose ``"summarize"``.  Falls back to a deterministic stub
(``_stub_summary``) on router error.
"""

import logging

from server.llm_router.client import complete as llm_complete

logger = logging.getLogger(__name__)

_MAX_SUMMARY_CHARS = 500


async def run(cleaned_text: str) -> str:
    """Generate a plain-language summary (<= 500 chars) via LLM router.

    Falls back to ``_stub_summary`` if the router is unreachable, returns
    an error, or the response is empty.

    Args:
        cleaned_text: Text extracted by the clean stage.

    Returns:
        Summary string (<= 500 characters).
    """
    try:
        result = await llm_complete(
            "summarize",
            dynamic_inputs={"user_text": cleaned_text[:4000]},
        )
        summary = result.get("text", "").strip()
        if summary:
            return summary[:_MAX_SUMMARY_CHARS]
    except Exception:
        logger.debug("summarize: router call failed, falling back to stub")
    # Fallback: deterministic stub
    return _stub_summary(cleaned_text)


def _stub_summary(cleaned_text: str) -> str:
    """Deterministic stub summary — first ``_MAX_SUMMARY_CHARS`` characters.

    Used as fallback when the LLM router is unavailable.

    Args:
        cleaned_text: Text to summarize.

    Returns:
        Truncated text (<= ``_MAX_SUMMARY_CHARS`` chars), or empty string
        if input is empty.
    """
    text = cleaned_text.strip()
    if not text:
        return ""
    return text[:_MAX_SUMMARY_CHARS]
