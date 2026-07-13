"""Pipeline stage 4: extract — entity, date, and claim extraction.

Calls the LLM Router (``server.llm_router.client.complete``) with
purpose ``"extract"``.  Expects JSON response with keys ``entities``,
``dates``, ``claims``.  Falls back to a regex-based stub
(``_stub_extract``) on router error, which parses:
- ISO-8601 dates (``YYYY-MM-DD``, ``YYYY-MM-DDTHH:MM:SS``)
- Capitalised phrases (3+ consecutive capitalised words)
- Explicit tickers (``$AAPL`` style)
- Simple possessive-entity patterns (``X's Y``)
"""

import json
import logging
import re

from server.llm_router.client import complete as llm_complete

logger = logging.getLogger(__name__)

# ISO-8601 date patterns
_RE_ISO_DATE = re.compile(
    r"\b(\d{4}-\d{2}-\d{2}(?:T\d{2}:\d{2}:\d{2}(?:[.]\d+)?(?:Z|[+-]\d{2}:?\d{2})?)?)\b"
)

# Capitalised phrases: 3+ consecutive words starting with uppercase
_RE_CAP_PHRASE = re.compile(r"\b([A-Z][a-z]+(?:\s+[A-Z][a-z]+){2,})\b")

# Explicit tickers: $ followed by 1-5 uppercase letters
_RE_TICKER = re.compile(r"\$([A-Z]{1,5})\b")

# Simple possessive entity: X's Y where X is capitalized
_RE_POSSESSIVE = re.compile(r"\b([A-Z][a-z]+)'s\s+([A-Za-z]+)")


async def run(cleaned_text: str) -> dict:
    """Extract entities, dates, claims via LLM router.

    Falls back to ``_stub_extract`` if the router is unreachable or returns
    unparseable JSON.

    Args:
        cleaned_text: Cleaned text from the clean stage.

    Returns:
        Dict with keys:
        - ``entities``: List of extracted entity strings (deduplicated).
        - ``dates``: Dict mapping ISO date string to context label.
        - ``claims``: List of extracted claim strings.
    """
    try:
        result = await llm_complete(
            "extract",
            dynamic_inputs={"user_text": cleaned_text[:4000]},
        )
        text = result.get("text", "")
        try:
            parsed = json.loads(text)
            if isinstance(parsed, dict):
                return parsed
        except json.JSONDecodeError:
            pass
    except Exception:
        logger.debug("extract: router call failed, falling back to stub")
    # Fallback: regex stub
    return _stub_extract(cleaned_text)


def _stub_extract(text: str) -> dict:
    """Deterministic regex-based extraction stub.

    Used as fallback when the LLM router is unavailable.

    Args:
        text: Cleaned text to extract from.

    Returns:
        Dict with keys ``entities``, ``dates``, ``claims`` (claims always
        empty for the stub).
    """
    if not text or not text.strip():
        return {"entities": [], "dates": {}, "claims": []}

    entities: list[str] = []
    dates: dict[str, str] = {}
    claims: list[str] = []

    # Extract ISO dates
    for match in _RE_ISO_DATE.finditer(text):
        date_str = match.group(1)
        label = _date_label(date_str)
        if date_str not in dates:
            dates[date_str] = label

    # Extract capitalised phrases as entities
    for match in _RE_CAP_PHRASE.finditer(text):
        phrase = match.group(1).strip()
        if phrase not in entities:
            entities.append(phrase)

    # Extract tickers
    for match in _RE_TICKER.finditer(text):
        ticker = f"${match.group(1)}"
        if ticker not in entities:
            entities.append(ticker)

    # Extract possessive entities
    for match in _RE_POSSESSIVE.finditer(text):
        entity = f"{match.group(1)}'s {match.group(2)}"
        if entity not in entities:
            entities.append(entity)

    # Claims: stub returns empty list
    # Actual claim extraction deferred to EP-202

    logger.debug(
        "extract: found %d entities, %d dates, %d claims",
        len(entities),
        len(dates),
        len(claims),
    )
    return {"entities": entities, "dates": dates, "claims": claims}


def _date_label(date_str: str) -> str:
    """Return a label for an ISO date string.

    If the string includes time, label as ``"timestamp"``,
    otherwise ``"date"``.
    """
    if "T" in date_str:
        return "timestamp"
    return "date"
