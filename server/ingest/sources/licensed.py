"""Rung 2: licensed feed adapter."""

import httpx

from server.ingest.models import LadderRung
from server.ingest.sources.http_json import HeaderProvider, JsonFeedAdapter


class LicensedFeedAdapter(JsonFeedAdapter):
    def __init__(
        self,
        *,
        source: str,
        endpoint: str,
        client: httpx.AsyncClient,
        credentials: HeaderProvider,
    ) -> None:
        super().__init__(
            source=source,
            endpoint=endpoint,
            client=client,
            rung=LadderRung.licensed_feed,
            headers=credentials,
        )
