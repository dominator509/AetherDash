"""Rung 3 RSS, Atom, and sitemap ingestion without active content crawling."""

import xml.etree.ElementTree as ET

import httpx

from server.ingest.models import FetchBatch, FetchedItem, LadderRung
from server.ingest.sources.http_json import validate_endpoint


def _local_name(tag: str) -> str:
    return tag.rsplit("}", 1)[-1].lower()


def _child_text(element: ET.Element, names: set[str]) -> str:
    for child in element.iter():
        if _local_name(child.tag) in names and child.text:
            return child.text.strip()
    return ""


class RssSitemapAdapter:
    rung = LadderRung.rss_or_sitemap

    def __init__(
        self,
        *,
        source: str,
        url: str,
        client: httpx.AsyncClient,
        max_response_bytes: int = 5_000_000,
    ) -> None:
        validate_endpoint(url)
        self.source = source
        self.url = url
        self.client = client
        self.max_response_bytes = max_response_bytes

    async def fetch(self, cursor: str | None, limit: int) -> FetchBatch:
        response = await self.client.get(self.url)
        response.raise_for_status()
        raw = response.content
        if len(raw) > self.max_response_bytes:
            raise ValueError("XML source exceeds configured byte limit")
        upper = raw.upper()
        if b"<!DOCTYPE" in upper or b"<!ENTITY" in upper:
            raise ValueError("DTD and entity declarations are prohibited")
        root = ET.fromstring(raw)
        is_sitemap = _local_name(root.tag) in {"urlset", "sitemapindex"}
        candidates = [
            element
            for element in root.iter()
            if _local_name(element.tag)
            in ({"url", "sitemap"} if is_sitemap else {"item", "entry"})
        ]
        items: list[FetchedItem] = []
        newest: str | None = None
        for element in candidates:
            link = _child_text(element, {"link", "loc", "id"})
            if not link:
                for child in element.iter():
                    if _local_name(child.tag) == "link" and child.attrib.get("href"):
                        link = child.attrib["href"].strip()
                        break
            if not link:
                continue
            newest = newest or link
            if cursor is not None and link == cursor:
                break
            if is_sitemap:
                content = link
                kind = "document"
            else:
                title = _child_text(element, {"title"})
                body = _child_text(element, {"description", "summary", "content"})
                content = "\n".join(value for value in (title, body, link) if value)
                kind = "news"
            items.append(
                FetchedItem(
                    kind=kind,
                    content=content,
                    raw_content=ET.tostring(element, encoding="utf-8"),
                    source=self.source,
                    trust="high",
                )
            )
            if len(items) >= limit:
                break
        return FetchBatch(items=tuple(items), next_cursor=newest or cursor)
