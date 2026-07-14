"""Shared fixtures and configuration for alert tests."""

import pytest

from server.alerts.rules import Rule, _reset_state


@pytest.fixture(autouse=True)
def reset_globals() -> None:
    """Reset shared dedup / rate-limit state before every test."""
    _reset_state()


@pytest.fixture
def sample_rule() -> Rule:
    return Rule(name="test_rule", kind_filter=["arbitrage"])


@pytest.fixture
def sample_opportunity() -> dict:
    return {
        "id": "01ARZ3NDEKTSV4RRFFQ69G5FAV",
        "kind": "arbitrage",
        "net_edge": "0.05",
        "confidence": 0.85,
        "venue": "kalshi",
        "market": "mkt:kalshi:BTC-75",
        "trace_id": "trace-001",
    }
