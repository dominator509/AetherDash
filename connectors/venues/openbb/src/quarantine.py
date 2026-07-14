"""Exact-byte quarantine sink for malformed OpenBB boundary payloads."""

from __future__ import annotations

import hashlib
import json
import os
import threading
from datetime import UTC, datetime
from pathlib import Path
from typing import Any


class QuarantineSink:
    """Persist raw bytes and an append-only `quarantine.openbb` outbox."""

    def __init__(self, root: Path | None = None) -> None:
        self._root = root or Path(
            os.environ.get("AETHER_QUARANTINE__DIR", "data/quarantine")
        )
        self._lock = threading.Lock()

    def preserve(self, reason: str, payload: Any) -> str:
        raw = json.dumps(payload, separators=(",", ":"), default=str).encode()
        digest = hashlib.sha256(raw).hexdigest()
        object_path = self._root / "openbb" / digest
        envelope = {
            "topic": "quarantine.openbb",
            "venue": "openbb",
            "reason": reason,
            "sha256": digest,
            "object_ref": str(object_path),
            "received_ts": datetime.now(UTC).isoformat(),
        }
        with self._lock:
            object_path.parent.mkdir(parents=True, exist_ok=True)
            if not object_path.exists():
                object_path.write_bytes(raw)
            outbox = self._root / "quarantine.openbb.jsonl"
            with outbox.open("a", encoding="utf-8") as handle:
                handle.write(json.dumps(envelope, separators=(",", ":")) + "\n")
        return digest
