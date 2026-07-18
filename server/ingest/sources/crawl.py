"""Rung 4 crawler with structural robots, origin, and rate-limit enforcement."""

import asyncio
import time
import urllib.robotparser
from collections.abc import Awaitable, Callable, Sequence
from html.parser import HTMLParser
from urllib.parse import urljoin, urlparse

import httpx

from server.ingest.models import FetchBatch, FetchedItem, LadderRung
from server.ingest.sources.http_json import validate_endpoint


class _TextExtractor(HTMLParser):
    def __init__(self) -> None:
        super().__init__()
        self.parts: list[str] = []

    def handle_data(self, data: str) -> None:
        value = data.strip()
        if value:
            self.parts.append(value)


class RobotsCrawlAdapter:
    rung = LadderRung.robots_compliant_crawl

    def __init__(
        self,
        *,
        source: str,
        base_url: str,
        paths: Sequence[str],
        client: httpx.AsyncClient,
        user_agent: str,
        min_interval_seconds: float,
        clock: Callable[[], float] = time.monotonic,
        sleeper: Callable[[float], Awaitable[None]] = asyncio.sleep,
        max_response_bytes: int = 5_000_000,
    ) -> None:
        validate_endpoint(base_url)
        if not user_agent.strip():
            raise ValueError("crawler requires a declared user agent")
        if min_interval_seconds < 1:
            raise ValueError("crawler rate limit must be at least one second")
        if any(not path.startswith("/") or urlparse(path).netloc for path in paths):
            raise ValueError("crawl paths must be same-origin absolute paths")
        self.source = source
        self.base_url = base_url.rstrip("/") + "/"
        self.paths = tuple(paths)
        self.client = client
        self.user_agent = user_agent
        self.min_interval_seconds = min_interval_seconds
        self.clock = clock
        self.sleeper = sleeper
        self.max_response_bytes = max_response_bytes
        self._next_request_at = 0.0

    async def _rate_limit(self) -> None:
        delay = self._next_request_at - self.clock()
        if delay > 0:
            await self.sleeper(delay)
        self._next_request_at = self.clock() + self.min_interval_seconds

    async def fetch(self, cursor: str | None, limit: int) -> FetchBatch:
        robots_url = urljoin(self.base_url, "/robots.txt")
        robots_response = await self.client.get(
            robots_url, headers={"User-Agent": self.user_agent}
        )
        if robots_response.status_code != 200 or robots_response.history:
            raise ValueError("robots.txt is unavailable or redirected; crawl refused")
        parser = urllib.robotparser.RobotFileParser()
        parser.set_url(robots_url)
        parser.parse(robots_response.text.splitlines())

        start = int(cursor) if cursor else 0
        items: list[FetchedItem] = []
        next_index = start
        expected_origin = urlparse(self.base_url).netloc
        for index, path in enumerate(self.paths[start:], start=start):
            next_index = index + 1
            url = urljoin(self.base_url, path)
            if urlparse(url).netloc != expected_origin:
                raise ValueError("crawl target escaped configured origin")
            if not parser.can_fetch(self.user_agent, url):
                continue
            await self._rate_limit()
            response = await self.client.get(
                url, headers={"User-Agent": self.user_agent}
            )
            if (
                response.history
                or urlparse(str(response.url)).netloc != expected_origin
            ):
                raise ValueError("crawl redirect escaped configured origin")
            response.raise_for_status()
            if len(response.content) > self.max_response_bytes:
                raise ValueError("crawl response exceeds configured byte limit")
            extractor = _TextExtractor()
            extractor.feed(response.text)
            items.append(
                FetchedItem(
                    kind="document",
                    content="\n".join(extractor.parts),
                    raw_content=response.content,
                    source=self.source,
                )
            )
            if len(items) >= limit:
                break
        return FetchBatch(items=tuple(items), next_cursor=str(next_index))
