"""Record scrubbed OpenBB quote responses as deterministic JSONL fixtures."""

from __future__ import annotations

import json
import os
from datetime import UTC, datetime
from pathlib import Path

from .client import OpenbbClient


def record_quotes(destination: Path, client: OpenbbClient | None = None) -> int:
    """Record one quote per configured symbol without credentials or headers."""
    sdk = client or OpenbbClient()
    frames = []
    raw_watchlist = os.environ.get(
        "AETHER_VENUE__OPENBB_WATCHLIST", "SPY,QQQ,AAPL,MSFT"
    )
    for symbol in [
        item.strip().upper() for item in raw_watchlist.split(",") if item.strip()
    ]:
        payload = sdk.get_quote(symbol)
        if payload:
            frames.append(
                {
                    "received_ts": datetime.now(UTC).isoformat(),
                    "symbol": symbol,
                    "payload": payload,
                }
            )
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(
        "".join(json.dumps(frame, separators=(",", ":")) + "\n" for frame in frames),
        encoding="utf-8",
    )
    return len(frames)


if __name__ == "__main__":
    target = Path("testdata/openbb/quotes_recording.jsonl")
    print(f"recorded {record_quotes(target)} OpenBB quotes to {target}")
