"""Rung 5: operator-authorized session adapter."""

import httpx

from server.ingest.models import LadderRung
from server.ingest.sources.http_json import HeaderProvider, JsonFeedAdapter


class AuthorizedSessionAdapter(JsonFeedAdapter):
    def __init__(
        self,
        *,
        source: str,
        endpoint: str,
        client: httpx.AsyncClient,
        session_headers: HeaderProvider,
    ) -> None:
        super().__init__(
            source=source,
            endpoint=endpoint,
            client=client,
            rung=LadderRung.user_authorized_session,
            headers=session_headers,
        )
