"""Fleet runtime wiring from secret-free JSON source configuration."""

import asyncio
import json
import os
from pathlib import Path

import asyncpg
import httpx
import structlog

from server.ingest.models import RuntimeSourceConfig
from server.ingest.ocr.pipeline import OcrPipeline, build_ocr_engine
from server.ingest.scheduler import FleetScheduler, SourceAdapter
from server.ingest.sources.crawl import RobotsCrawlAdapter
from server.ingest.sources.licensed import LicensedFeedAdapter
from server.ingest.sources.manual import ManualReviewAdapter, PostgresManualReviewQueue
from server.ingest.sources.official_api import OfficialApiAdapter
from server.ingest.sources.rss import RssSitemapAdapter
from server.ingest.sources.session import AuthorizedSessionAdapter
from server.ingest.state import PostgresStateStore

logger = structlog.get_logger().bind(service="ingest", plane="brain")


def load_source_configs(path: str | Path) -> list[RuntimeSourceConfig]:
    data = json.loads(Path(path).read_text(encoding="utf-8"))
    if not isinstance(data, list):
        raise ValueError("ingestion source config must be a JSON array")
    configs = [RuntimeSourceConfig.model_validate(item) for item in data]
    sources = [config.source for config in configs]
    if len(sources) != len(set(sources)):
        raise ValueError("ingestion source names must be unique")
    return configs


def _credential_provider(mapping: dict[str, str]):
    def provide() -> dict[str, str]:
        headers: dict[str, str] = {}
        for header, variable in mapping.items():
            value = os.environ.get(variable)
            if not value:
                raise RuntimeError("configured source credential is unavailable")
            headers[header] = value
        return headers

    return provide


def build_adapter(
    config: RuntimeSourceConfig,
    *,
    client: httpx.AsyncClient,
    manual_queue: PostgresManualReviewQueue,
) -> SourceAdapter:
    common = {"source": config.source, "client": client}
    if config.adapter == "official_api":
        return OfficialApiAdapter(
            **common,
            endpoint=config.endpoint or "",
            headers=_credential_provider(config.credential_headers),
        )
    if config.adapter == "licensed_feed":
        return LicensedFeedAdapter(
            **common,
            endpoint=config.endpoint or "",
            credentials=_credential_provider(config.credential_headers),
        )
    if config.adapter == "rss_or_sitemap":
        return RssSitemapAdapter(
            **common,
            url=config.endpoint or "",
        )
    if config.adapter == "robots_compliant_crawl":
        return RobotsCrawlAdapter(
            **common,
            base_url=config.endpoint or "",
            paths=config.paths,
            user_agent=config.user_agent or "",
            min_interval_seconds=config.min_interval_seconds or 0,
        )
    if config.adapter == "user_authorized_session":
        return AuthorizedSessionAdapter(
            **common,
            endpoint=config.endpoint or "",
            session_headers=_credential_provider(config.credential_headers),
        )
    return ManualReviewAdapter(source=config.source, repository=manual_queue)


class FleetRuntime:
    def __init__(
        self,
        pool: asyncpg.Pool,
        configs: list[RuntimeSourceConfig],
        *,
        workers: int = 4,
    ) -> None:
        if not 1 <= workers <= 64:
            raise ValueError("ingestion worker count must be between 1 and 64")
        self.pool = pool
        self.configs = configs
        self.workers = workers
        self.client = httpx.AsyncClient(timeout=30, follow_redirects=False)
        self.scheduler = FleetScheduler(PostgresStateStore(pool))
        self.ocr = (
            OcrPipeline(build_ocr_engine())
            if os.environ.get("AETHER_INGEST__OCR_ENABLED", "1") == "1"
            else None
        )
        self.ocr_interval = max(
            1.0, float(os.environ.get("AETHER_INGEST__OCR_INTERVAL_SECONDS", "5"))
        )
        self.stop = asyncio.Event()
        self.tasks: list[asyncio.Task[None]] = []

    async def start(self) -> None:
        manual_queue = PostgresManualReviewQueue(self.pool)
        for config in self.configs:
            adapter = build_adapter(
                config, client=self.client, manual_queue=manual_queue
            )
            await self.scheduler.register(
                config.scheduler_config(),
                adapter,
                downgrade=config.downgrade,
            )
        self.tasks = [
            asyncio.create_task(self._poll_loop(), name="ingest-poller"),
            *(
                asyncio.create_task(
                    self.scheduler.worker(self.stop), name=f"ingest-worker-{index}"
                )
                for index in range(self.workers)
            ),
        ]
        if self.ocr is not None:
            self.tasks.append(
                asyncio.create_task(self._ocr_loop(), name="ingest-ocr-worker")
            )

    async def _poll_loop(self) -> None:
        while not self.stop.is_set():
            await self.scheduler.poll_due()
            try:
                await asyncio.wait_for(self.stop.wait(), timeout=0.5)
            except TimeoutError:
                continue

    async def _ocr_loop(self) -> None:
        assert self.ocr is not None
        while not self.stop.is_set():
            for obj in await self.ocr.pending_screenshots():
                try:
                    await self.ocr.reprocess_existing(obj.id)
                except Exception as exc:
                    logger.warning(
                        "ocr_reprocessing_failed",
                        object_id=obj.id,
                        error_class=type(exc).__name__,
                    )
            try:
                await asyncio.wait_for(self.stop.wait(), timeout=self.ocr_interval)
            except TimeoutError:
                continue

    def healthy(self) -> bool:
        return bool(self.tasks) and all(not task.done() for task in self.tasks)

    async def close(self) -> None:
        self.stop.set()
        await asyncio.gather(*self.tasks, return_exceptions=True)
        self.tasks.clear()
        await self.client.aclose()
