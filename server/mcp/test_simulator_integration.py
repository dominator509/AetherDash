"""Real canonical simulator transport proof (requires the Rust binary)."""

from pathlib import Path

import pytest
from simulator import run_simulation


@pytest.mark.integration
@pytest.mark.asyncio
async def test_simulator_binary_round_trip(monkeypatch: pytest.MonkeyPatch) -> None:
    binary = Path("target/debug/aether-simulator.exe")
    if not binary.exists():
        pytest.skip("build aether-simulator before the transport integration test")
    monkeypatch.setenv("AETHER_SIMULATOR_BIN", str(binary))
    result = await run_simulation(
        {
            "buy_price": "0.62",
            "sell_price": "0.67",
            "price_kind": "probability",
            "notional": "1",
            "buy_book": {
                "market": "mkt:kalshi:BTC",
                "bids": [{"price": "0.61", "size": "10"}],
                "asks": [{"price": "0.62", "size": "10"}],
                "depth": 1,
                "ts": "2026-07-17T12:00:00.000Z",
            },
            "sell_book": {
                "market": "mkt:polymarket:BTC",
                "bids": [{"price": "0.67", "size": "10"}],
                "asks": [{"price": "0.68", "size": "10"}],
                "depth": 1,
                "ts": "2026-07-17T12:00:00.000Z",
            },
            "funding_rate": "0",
            "hold_hours": "0",
            "max_quote_age_ms": 0,
            "tick_stale_ms": 5000,
            "confidence": "1",
            "is_cross_chain": False,
            "buy_venue": "kalshi",
            "sell_venue": "polymarket",
        }
    )
    assert result["decomposition"]["gross_spread"] == "0.05"
    assert "net_edge" in result["decomposition"]
    assert result["buy_fills"]
    assert result["sell_fills"]
    assert result["sensitivity"]["rows"]
