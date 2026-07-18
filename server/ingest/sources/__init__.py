"""Compliance-ladder source adapters."""

from server.ingest.sources.crawl import RobotsCrawlAdapter
from server.ingest.sources.licensed import LicensedFeedAdapter
from server.ingest.sources.manual import ManualReviewAdapter, PostgresManualReviewQueue
from server.ingest.sources.official_api import OfficialApiAdapter
from server.ingest.sources.rss import RssSitemapAdapter
from server.ingest.sources.session import AuthorizedSessionAdapter

__all__ = [
    "AuthorizedSessionAdapter",
    "LicensedFeedAdapter",
    "ManualReviewAdapter",
    "OfficialApiAdapter",
    "PostgresManualReviewQueue",
    "RobotsCrawlAdapter",
    "RssSitemapAdapter",
]
