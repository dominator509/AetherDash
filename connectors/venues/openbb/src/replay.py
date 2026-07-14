"""Deterministic replay for scrubbed OpenBB quote recordings."""

from __future__ import annotations

import json
from typing import Any

from .normalize import CanonicalQuote, normalize_quote


def replay_jsonl(recording: str) -> list[CanonicalQuote]:
    """Normalize a JSONL recording using recorded receive time as fallback."""
    quotes: list[CanonicalQuote] = []
    for line_number, line in enumerate(recording.splitlines(), start=1):
        if not line.strip():
            continue
        try:
            frame: dict[str, Any] = json.loads(line)
            received_ts = frame["received_ts"]
            symbol = frame["symbol"]
            payload = dict(frame["payload"])
        except (KeyError, TypeError, json.JSONDecodeError) as exc:
            raise ValueError(f"invalid OpenBB recording line {line_number}") from exc
        payload.setdefault("timestamp", received_ts)
        quote = normalize_quote(payload, symbol)
        if quote is None:
            raise ValueError(f"empty OpenBB quote at recording line {line_number}")
        quotes.append(quote)
    return quotes
