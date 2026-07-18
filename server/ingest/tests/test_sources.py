import json

import httpx
import pytest

from server.ingest.models import FetchedItem, LadderRung
from server.ingest.sources.crawl import RobotsCrawlAdapter
from server.ingest.sources.licensed import LicensedFeedAdapter
from server.ingest.sources.manual import ManualReviewAdapter
from server.ingest.sources.official_api import OfficialApiAdapter
from server.ingest.sources.rss import RssSitemapAdapter
from server.ingest.sources.session import AuthorizedSessionAdapter


def json_response(request: httpx.Request) -> httpx.Response:
    assert request.headers.get("x-api-key") == "credential"
    return httpx.Response(
        200,
        json={
            "items": [{"kind": "news", "content": "verified", "trust": "high"}],
            "next_cursor": "page-2",
        },
    )


@pytest.mark.asyncio
@pytest.mark.parametrize(
    ("adapter_kind", "expected_rung"),
    [
        ("official", LadderRung.official_api),
        ("licensed", LadderRung.licensed_feed),
        ("session", LadderRung.user_authorized_session),
    ],
)
async def test_json_source_rungs_force_registered_source_identity(
    adapter_kind: str, expected_rung: LadderRung
) -> None:
    async with httpx.AsyncClient(
        transport=httpx.MockTransport(json_response)
    ) as client:
        common = {
            "source": f"{adapter_kind}:fixture",
            "endpoint": "https://example.test/feed",
            "client": client,
        }
        if adapter_kind == "official":
            adapter = OfficialApiAdapter(
                **common, headers=lambda: {"x-api-key": "credential"}
            )
        elif adapter_kind == "licensed":
            adapter = LicensedFeedAdapter(
                **common, credentials=lambda: {"x-api-key": "credential"}
            )
        else:
            adapter = AuthorizedSessionAdapter(
                **common, session_headers=lambda: {"x-api-key": "credential"}
            )
        batch = await adapter.fetch(None, 10)

    assert adapter.rung == expected_rung
    assert batch.next_cursor == "page-2"
    assert batch.items[0].source == f"{adapter_kind}:fixture"
    assert json.loads(batch.items[0].raw_content)["content"] == "verified"


@pytest.mark.asyncio
async def test_rss_parser_rejects_entity_declarations() -> None:
    def handler(_: httpx.Request) -> httpx.Response:
        return httpx.Response(
            200,
            content=b'<!DOCTYPE rss [<!ENTITY xxe SYSTEM "file:///etc/passwd">]><rss/>',
        )

    async with httpx.AsyncClient(transport=httpx.MockTransport(handler)) as client:
        adapter = RssSitemapAdapter(
            source="rss:fixture", url="https://example.test/rss", client=client
        )
        with pytest.raises(ValueError, match="entity declarations"):
            await adapter.fetch(None, 10)


@pytest.mark.asyncio
async def test_rss_feed_emits_news_without_fetching_linked_pages() -> None:
    calls: list[str] = []

    def handler(request: httpx.Request) -> httpx.Response:
        calls.append(request.url.path)
        return httpx.Response(
            200,
            content=(
                b"<rss><channel><item><title>Headline</title>"
                b"<description>Body</description><link>https://news.test/1</link>"
                b"</item></channel></rss>"
            ),
        )

    async with httpx.AsyncClient(transport=httpx.MockTransport(handler)) as client:
        adapter = RssSitemapAdapter(
            source="rss:fixture", url="https://example.test/rss", client=client
        )
        batch = await adapter.fetch(None, 10)

    assert calls == ["/rss"]
    assert batch.items[0].kind == "news"
    assert "Headline" in batch.items[0].content


@pytest.mark.asyncio
async def test_robots_respect_disallowed_path_is_never_requested() -> None:
    calls: list[str] = []

    def handler(request: httpx.Request) -> httpx.Response:
        calls.append(request.url.path)
        if request.url.path == "/robots.txt":
            return httpx.Response(200, text="User-agent: *\nDisallow: /private\n")
        if request.url.path == "/public":
            return httpx.Response(200, text="<html><body>Public report</body></html>")
        raise AssertionError(f"disallowed request was sent: {request.url.path}")

    async with httpx.AsyncClient(transport=httpx.MockTransport(handler)) as client:
        adapter = RobotsCrawlAdapter(
            source="crawl:fixture",
            base_url="https://example.test",
            paths=["/private", "/public"],
            client=client,
            user_agent="AetherDashComplianceBot/1.0",
            min_interval_seconds=1,
        )
        batch = await adapter.fetch(None, 10)

    assert calls == ["/robots.txt", "/public"]
    assert batch.items[0].content == "Public report"
    assert batch.next_cursor == "2"


@pytest.mark.asyncio
async def test_crawl_enforces_declared_rate_limit_between_pages() -> None:
    now = [0.0]
    sleeps: list[float] = []

    async def sleep(delay: float) -> None:
        sleeps.append(delay)
        now[0] += delay

    def handler(request: httpx.Request) -> httpx.Response:
        if request.url.path == "/robots.txt":
            return httpx.Response(200, text="User-agent: *\nAllow: /\n")
        return httpx.Response(200, text=f"<p>{request.url.path}</p>")

    async with httpx.AsyncClient(transport=httpx.MockTransport(handler)) as client:
        adapter = RobotsCrawlAdapter(
            source="crawl:fixture",
            base_url="https://example.test",
            paths=["/one", "/two"],
            client=client,
            user_agent="AetherDashComplianceBot/1.0",
            min_interval_seconds=2,
            clock=lambda: now[0],
            sleeper=sleep,
        )
        batch = await adapter.fetch(None, 10)

    assert len(batch.items) == 2
    assert sleeps == [2.0]


class FakeManualRepository:
    async def approved_after(
        self, source: str, cursor: str | None, limit: int
    ) -> tuple[tuple[str, FetchedItem], ...]:
        assert cursor is None
        assert limit == 5
        return (
            (
                "01ARZ3NDEKTSV4RRFFQ69G5FAV",
                FetchedItem(
                    kind="note",
                    content="operator approved",
                    raw_content=b"operator approved",
                    source=source,
                ),
            ),
        )


@pytest.mark.asyncio
async def test_manual_review_adapter_only_reads_repository_approved_items() -> None:
    adapter = ManualReviewAdapter(
        source="manual:fixture", repository=FakeManualRepository()
    )
    batch = await adapter.fetch(None, 5)
    assert adapter.rung == LadderRung.manual_review
    assert batch.items[0].content == "operator approved"
    assert batch.next_cursor == "01ARZ3NDEKTSV4RRFFQ69G5FAV"
