import json
import os
from pathlib import Path
from typing import Any

import pytest
from fastapi.testclient import TestClient

from server.ingest import app as app_module
from server.ingest.metrics import render_metrics
from server.ingest.models import LadderRung
from server.ingest.runtime import load_source_configs


class FakePool:
    async def fetch(self, query: str, *args: object) -> list[dict[str, Any]]:
        if "GROUP BY source,ladder_rung" in query:
            return [
                {
                    "source": 'official:"fixture"',
                    "ladder_rung": 1,
                    "count": 3,
                }
            ]
        if "FROM ingest_source_state" in query:
            return [
                {
                    "source": "official:fixture",
                    "ladder_rung": 1,
                    "health": "healthy",
                    "consecutive_failures": 0,
                }
            ]
        if "FROM ingest_source_events" in query:
            return [
                {
                    "object_id": "01ARZ3NDEKTSV4RRFFQ69G5FAV",
                    "source": "official:fixture",
                    "ladder_rung": 1,
                    "bytes": 12,
                    "status": "ingested",
                    "trace_id": "01ARZ3NDEKTSV4RRFFQ69G5FAW",
                    "created_ts": "2026-07-18T00:00:00Z",
                }
            ]
        if "FROM ingest_rung_decisions" in query:
            return []
        raise AssertionError(query)

    async def fetchval(self, query: str) -> int:
        assert query == "SELECT 1"
        return 1


class HealthyRuntime:
    def healthy(self) -> bool:
        return True


def test_runtime_config_is_secret_free_and_declares_exact_rung(tmp_path: Path) -> None:
    path = tmp_path / "sources.json"
    path.write_text(
        json.dumps(
            [
                {
                    "source": "licensed:fixture",
                    "rung": 2,
                    "adapter": "licensed_feed",
                    "interval_seconds": 60,
                    "endpoint": "https://example.test/feed",
                    "credential_headers": {"Authorization": "FIXTURE_TOKEN_ENV"},
                }
            ]
        ),
        encoding="utf-8",
    )
    configs = load_source_configs(path)
    assert configs[0].rung == LadderRung.licensed_feed
    assert configs[0].credential_headers == {"Authorization": "FIXTURE_TOKEN_ENV"}
    assert "token-value" not in path.read_text(encoding="utf-8")


def test_runtime_config_rejects_adapter_rung_mismatch(tmp_path: Path) -> None:
    path = tmp_path / "sources.json"
    path.write_text(
        json.dumps(
            [
                {
                    "source": "rss:fixture",
                    "rung": 1,
                    "adapter": "rss_or_sitemap",
                    "interval_seconds": 60,
                    "endpoint": "https://example.test/rss",
                }
            ]
        ),
        encoding="utf-8",
    )
    with pytest.raises(ValueError, match="adapter does not match"):
        load_source_configs(path)


@pytest.mark.asyncio
async def test_metrics_include_required_rung_and_source_health_series() -> None:
    text = await render_metrics(FakePool())  # type: ignore[arg-type]
    assert (
        'aether_ingest_objects_total{source="official:\\"fixture\\"",ladder_rung="1"} 3'
        in text
    )
    assert (
        'aether_ingest_source_healthy{source="official:fixture",ladder_rung="1"} 1'
        in text
    )
    assert 'aether_build_info{service="ingest",version="0.1.0"} 1' in text


@pytest.mark.asyncio
async def test_health_readiness_and_audit_surfaces(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    pool = FakePool()
    monkeypatch.setattr(app_module, "_pool", pool)
    monkeypatch.setattr(app_module, "_runtime", HealthyRuntime())

    assert await app_module.healthz() == {"status": "ok", "service": "ingest"}
    assert await app_module.readyz() == {"status": "ok", "service": "ingest"}
    audit = await app_module.source_audit(100)
    assert audit["events"][0]["ladder_rung"] == 1
    assert audit["events"][0]["trace_id"] == "01ARZ3NDEKTSV4RRFFQ69G5FAW"


@pytest.mark.integration
@pytest.mark.skipif(
    os.environ.get("AETHER_INTEGRATION_TEST") != "1",
    reason="requires a migrated disposable Postgres database",
)
def test_live_service_lifespan_and_operator_surfaces(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    config_path = tmp_path / "sources.json"
    config_path.write_text(
        json.dumps(
            [
                {
                    "source": "manual:service-integration",
                    "rung": 6,
                    "adapter": "manual_review",
                    "interval_seconds": 60,
                    "enabled": True,
                }
            ]
        ),
        encoding="utf-8",
    )
    monkeypatch.setenv("AETHER_INGEST__CONFIG_PATH", str(config_path))
    monkeypatch.setenv("AETHER_INGEST__OCR_ENABLED", "0")
    monkeypatch.setenv("AETHER_INGEST__WORKERS", "1")

    with TestClient(app_module.app) as client:
        assert client.get("/healthz").status_code == 200
        assert client.get("/readyz").status_code == 200
        metrics = client.get("/metrics")
        assert metrics.status_code == 200
        assert "aether_ingest_source_healthy" in metrics.text
        audit = client.get("/audit/sources")
        assert audit.status_code == 200
        assert set(audit.json()) == {"events", "downgrades"}
