"""Rung 1: official API adapter."""

import httpx

from server.ingest.models import LadderRung
from server.ingest.sources.http_json import HeaderProvider, JsonFeedAdapter


class OfficialApiAdapter(JsonFeedAdapter):
    def __init__(
        self,
        *,
        source: str,
        endpoint: str,
        client: httpx.AsyncClient,
        headers: HeaderProvider | None = None,
    ) -> None:
        super().__init__(
            source=source,
            endpoint=endpoint,
            client=client,
            rung=LadderRung.official_api,
            headers=headers,
        )
