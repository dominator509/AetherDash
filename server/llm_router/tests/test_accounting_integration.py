"""Live ClickHouse accounting acceptance test for EP-202."""

from uuid import uuid4

import httpx
import pytest

from server.llm_router.accounting import (
    CLICKHOUSE_PASSWORD,
    CLICKHOUSE_URL,
    CLICKHOUSE_USER,
    record_call,
)


@pytest.mark.integration
@pytest.mark.asyncio
async def test_every_call_writes_llm_calls_row() -> None:
    trace_id = f"ep202-{uuid4().hex}"
    await record_call(
        purpose="integration",
        provider="local",
        model="stub",
        prompt_tokens=1,
        completion_tokens=1,
        cache_hit=False,
        cost_usd=0.0,
        latency_ms=1.0,
        trace_id=trace_id,
    )

    async with httpx.AsyncClient() as client:
        response = await client.get(
            CLICKHOUSE_URL,
            params={
                "query": (
                    "SELECT count() FROM aether.llm_calls "
                    f"WHERE trace_id = '{trace_id}'"
                )
            },
            auth=(CLICKHOUSE_USER, CLICKHOUSE_PASSWORD),
            timeout=5.0,
        )
    response.raise_for_status()
    assert response.text.strip() == "1"
