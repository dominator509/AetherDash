"""Strict JSON feed transport shared by API-shaped compliance rungs."""

import json
from collections.abc import Callable, Mapping
from urllib.parse import urlparse

import httpx

from server.ingest.models import FetchBatch, FetchedItem, LadderRung

HeaderProvider = Callable[[], Mapping[str, str]]


def validate_endpoint(url: str) -> None:
    parsed = urlparse(url)
    if parsed.scheme == "https":
        return
    if parsed.scheme == "http" and parsed.hostname in {"127.0.0.1", "localhost"}:
        return
    raise ValueError("ingestion endpoints require HTTPS except on loopback")


class JsonFeedAdapter:
    """Fetch a canonical ``{items, next_cursor}`` JSON feed."""

    rung: LadderRung

    def __init__(
        self,
        *,
        source: str,
        endpoint: str,
        client: httpx.AsyncClient,
        rung: LadderRung,
        headers: HeaderProvider | None = None,
        max_response_bytes: int = 5_000_000,
    ) -> None:
        validate_endpoint(endpoint)
        if max_response_bytes < 1:
            raise ValueError("max_response_bytes must be positive")
        self.source = source
        self.endpoint = endpoint
        self.client = client
        self.rung = rung
        self.headers = headers
        self.max_response_bytes = max_response_bytes

    async def fetch(self, cursor: str | None, limit: int) -> FetchBatch:
        response = await self.client.get(
            self.endpoint,
            params={"limit": limit, **({"cursor": cursor} if cursor else {})},
            headers=dict(self.headers()) if self.headers else None,
        )
        response.raise_for_status()
        if len(response.content) > self.max_response_bytes:
            raise ValueError("source response exceeds configured byte limit")
        payload = response.json()
        if not isinstance(payload, dict) or not isinstance(payload.get("items"), list):
            raise ValueError("source response must contain an items array")
        items: list[FetchedItem] = []
        for value in payload["items"][:limit]:
            if not isinstance(value, dict):
                raise ValueError("source item must be an object")
            kind = value.get("kind")
            content = value.get("content")
            trust = value.get("trust", "medium")
            if not isinstance(kind, str) or not isinstance(content, str):
                raise ValueError("source item kind and content must be strings")
            if trust not in {"low", "medium", "high"}:
                raise ValueError("source item trust is invalid")
            items.append(
                FetchedItem(
                    kind=kind,
                    content=content,
                    raw_content=json.dumps(
                        value, sort_keys=True, separators=(",", ":")
                    ).encode(),
                    source=self.source,
                    trust=trust,
                )
            )
        next_cursor = payload.get("next_cursor")
        if next_cursor is not None and not isinstance(next_cursor, str):
            raise ValueError("next_cursor must be a string or null")
        return FetchBatch(items=tuple(items), next_cursor=next_cursor)
