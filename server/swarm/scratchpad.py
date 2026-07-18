"""Concurrent append-only, deduplicated, size-bounded swarm scratchpad."""

import asyncio
import hashlib
from dataclasses import dataclass


@dataclass(frozen=True)
class ScratchEntry:
    worker_id: str
    text: str
    citation_ids: tuple[str, ...]
    digest: str


class Scratchpad:
    def __init__(self, *, max_entries: int = 256, max_bytes: int = 256_000) -> None:
        if max_entries < 1 or max_bytes < 1:
            raise ValueError("scratchpad bounds must be positive")
        self.max_entries = max_entries
        self.max_bytes = max_bytes
        self._entries: list[ScratchEntry] = []
        self._digests: set[str] = set()
        self._bytes = 0
        self._lock = asyncio.Lock()

    async def append(
        self, worker_id: str, text: str, citation_ids: tuple[str, ...]
    ) -> bool:
        normalized = " ".join(text.split())
        if not worker_id or not normalized or not citation_ids:
            raise ValueError("scratch entries require worker, text, and citations")
        digest = hashlib.sha256(normalized.encode("utf-8")).hexdigest()
        size = len(normalized.encode("utf-8"))
        async with self._lock:
            if digest in self._digests:
                return False
            if (
                len(self._entries) >= self.max_entries
                or self._bytes + size > self.max_bytes
            ):
                return False
            self._entries.append(
                ScratchEntry(
                    worker_id, normalized, tuple(dict.fromkeys(citation_ids)), digest
                )
            )
            self._digests.add(digest)
            self._bytes += size
            return True

    async def snapshot(self) -> tuple[ScratchEntry, ...]:
        async with self._lock:
            return tuple(self._entries)

    async def size_bytes(self) -> int:
        async with self._lock:
            return self._bytes
