"""Tests for the OpenBB venue adapter pack.

All tests mock the OpenBB SDK — no real API calls are made.
"""

from __future__ import annotations

import json
from typing import Any
from unittest.mock import MagicMock

import grpc
import pytest

from connectors.venues.openbb.src.client import OpenbbClient
from connectors.venues.openbb.src.normalize import (
    normalize_market_from_option_contract,
    normalize_market_from_profile,
    normalize_option_quote,
    normalize_quote,
)
from connectors.venues.openbb.src.quarantine import QuarantineSink
from connectors.venues.openbb.src.replay import replay_jsonl

# ======================================================================
# Fixtures
# ======================================================================


@pytest.fixture
def mock_obb() -> MagicMock:
    """Fixture: a mock OpenBB SDK object (``obb``)."""
    return MagicMock()


@pytest.fixture
def client(mock_obb: MagicMock) -> OpenbbClient:
    """Fixture: an ``OpenbbClient`` with a mocked OpenBB SDK."""
    return OpenbbClient(obb=mock_obb)


@pytest.fixture
def sample_quote() -> dict[str, Any]:
    """Sample quote data as returned by OpenBB yfinance provider."""
    return {
        "symbol": "AAPL",
        "asset_type": "equity",
        "name": "Apple Inc.",
        "exchange": "NMS",
        "bid": "245.30",
        "bid_size": "2",
        "ask": "245.35",
        "ask_size": "3",
        "last_price": "245.32",
        "last_trade_time": "2025-03-15T14:30:00Z",
        "volume": 45200000,
        "change": 1.25,
        "change_percent": 0.51,
    }


@pytest.fixture
def sample_profile() -> dict[str, Any]:
    """Sample company profile data."""
    return {
        "symbol": "AAPL",
        "name": "Apple Inc.",
        "exchange": "NASDAQ",
        "sector": "Technology",
        "industry": "Consumer Electronics",
        "market_cap": 3200000000000,
        "currency": "USD",
    }


@pytest.fixture
def sample_option_contract() -> dict[str, Any]:
    """Single option contract from the options chain."""
    return {
        "contract_symbol": "AAPL250919C00150000",
        "underlying_symbol": "AAPL",
        "expiration": "2025-09-19",
        "strike": 150.0,
        "option_type": "call",
        "contract_size": 100,
        "bid": "8.50",
        "bid_size": "10",
        "ask": "8.65",
        "ask_size": "15",
        "last_trade_price": "8.55",
        "last_trade_time": "2025-03-15T14:30:00Z",
        "open_interest": 425000,
        "volume": 12000,
        "implied_volatility": 0.285,
        "delta": 0.65,
        "gamma": 0.015,
        "theta": -0.08,
        "vega": 0.12,
        "rho": 0.04,
    }


# ======================================================================
# Client tests
# ======================================================================


def test_locked_environment_resolves_the_external_openbb_sdk() -> None:
    """The local adapter package must not shadow the installed SDK."""
    import openbb
    from openbb import obb

    module_path = str(openbb.__file__).replace("\\", "/")
    assert "/connectors/venues/openbb/" not in module_path
    assert obb is not None


def test_get_quote_success(client: OpenbbClient, sample_quote: dict[str, Any]) -> None:
    """get_quote returns unwrapped results on success."""
    mock_result = MagicMock()
    mock_result.results = [sample_quote]
    client._obb.equity.price.quote.return_value = mock_result

    result = client.get_quote("AAPL")

    assert result == sample_quote
    client._obb.equity.price.quote.assert_called_once_with("AAPL", provider="yfinance")


def test_get_quote_failure_returns_empty(client: OpenbbClient) -> None:
    """get_quote returns empty dict on failure."""
    client._obb.equity.price.quote.side_effect = RuntimeError("API down")

    result = client.get_quote("AAPL")
    assert result == {}


def test_get_reference_success(
    client: OpenbbClient, sample_profile: dict[str, Any]
) -> None:
    """get_reference returns unwrapped results on success."""
    mock_result = MagicMock()
    mock_result.results = [sample_profile]
    client._obb.equity.profile.return_value = mock_result

    result = client.get_reference("AAPL")

    assert result == sample_profile
    client._obb.equity.profile.assert_called_once_with("AAPL", provider="yfinance")


def test_get_options_chain_success(
    client: OpenbbClient, sample_option_contract: dict[str, Any]
) -> None:
    """get_options_chain returns a list of contract dicts."""
    mock_result = MagicMock()
    mock_result.results = [sample_option_contract]
    client._obb.derivatives.options.chains.return_value = mock_result

    result = client.get_options_chain("AAPL")

    assert len(result) == 1
    assert result[0]["contract_symbol"] == "AAPL250919C00150000"
    client._obb.derivatives.options.chains.assert_called_once_with(
        "AAPL", provider="yfinance"
    )


def test_get_options_chain_with_filters(
    client: OpenbbClient, sample_option_contract: dict[str, Any]
) -> None:
    """get_options_chain passes optional kwargs."""
    mock_result = MagicMock()
    mock_result.results = [sample_option_contract]
    client._obb.derivatives.options.chains.return_value = mock_result

    result = client.get_options_chain(
        "AAPL", expiration="2025-09-19", option_type="call"
    )

    assert len(result) == 1
    client._obb.derivatives.options.chains.assert_called_once_with(
        "AAPL", provider="yfinance", expiration="2025-09-19", option_type="call"
    )


def test_check_connectivity_success(client: OpenbbClient) -> None:
    """check_connectivity returns True when the quote succeeds."""
    mock_result = MagicMock()
    mock_result.results = [{"symbol": "SPY", "last_price": 500.0}]
    client._obb.equity.price.quote.return_value = mock_result

    assert client.check_connectivity() is True


def test_check_connectivity_failure(client: OpenbbClient) -> None:
    """check_connectivity returns False on exception."""
    client._obb.equity.price.quote.side_effect = RuntimeError("fail")

    assert client.check_connectivity() is False


# ======================================================================
# Normalizer tests — quote
# ======================================================================


def test_normalize_quote(sample_quote: dict[str, Any]) -> None:
    """Normalize a full quote dict."""
    result = normalize_quote(sample_quote, "AAPL")

    assert result is not None
    assert result["market"] == "mkt:openbb:aapl"
    assert result["bid"] == "245.30"
    assert result["ask"] == "245.35"
    assert result["mid"] == "245.325"
    assert result["last"] == "245.32"
    assert result["source"] == "poll"
    assert result["ts"] is not None


def test_normalize_quote_empty() -> None:
    """Empty input returns None."""
    assert normalize_quote({}, "AAPL") is None


def test_normalize_quote_no_bid_ask() -> None:
    """When bid/ask are missing, mid is None and last is still populated."""
    raw = {"symbol": "AAPL", "last_price": 150.0}
    result = normalize_quote(raw, "AAPL")

    assert result is not None
    assert result["last"] == "150.0"
    assert result["bid"] is None
    assert result["ask"] is None
    assert result["mid"] is None


def test_normalize_quote_rejects_malformed_decimal() -> None:
    with pytest.raises(ValueError, match="invalid decimal"):
        normalize_quote({"symbol": "AAPL", "bid": "not-a-price"}, "AAPL")


def test_recording_replay_is_deterministic() -> None:
    from pathlib import Path

    recording = (
        Path(__file__).resolve().parents[4]
        / "testdata"
        / "openbb"
        / "quotes_recording.jsonl"
    ).read_text(encoding="utf-8")
    first = replay_jsonl(recording)
    second = replay_jsonl(recording)
    assert first == second
    assert first[1]["ts"] == "2025-03-15T14:30:02+00:00"


def test_quarantine_preserves_exact_canonical_bytes(tmp_path: Any) -> None:
    sink = QuarantineSink(tmp_path)
    payload = {"symbol": "AAPL", "bid": "not-a-price"}
    digest = sink.preserve("invalid decimal", payload)
    expected = json.dumps(payload, separators=(",", ":")).encode()
    assert (tmp_path / "openbb" / digest).read_bytes() == expected
    outbox = (tmp_path / "quarantine.openbb.jsonl").read_text(encoding="utf-8")
    assert '"topic":"quarantine.openbb"' in outbox


# ======================================================================
# Normalizer tests — market from profile
# ======================================================================


def test_normalize_market_from_profile(sample_profile: dict[str, Any]) -> None:
    """Build a canonical Market from profile data."""
    result = normalize_market_from_profile("AAPL", sample_profile)

    assert result is not None
    assert result["key"] == "mkt:openbb:aapl"
    assert result["venue"] == "openbb"
    assert result["kind"] == "equity"
    assert result["title"] == "Apple Inc."
    assert result["status"] == "open"
    assert result["venue_ref"]["symbol"] == "AAPL"
    assert result["venue_ref"]["exchange"] == "NASDAQ"


def test_normalize_market_from_empty_profile() -> None:
    """Missing profile is not fabricated into a live equity market."""
    result = normalize_market_from_profile("AAPL", {})
    assert result is None


# ======================================================================
# Normalizer tests — option market
# ======================================================================


def test_normalize_option_market(sample_option_contract: dict[str, Any]) -> None:
    """Build a canonical Market from an option contract."""
    result = normalize_market_from_option_contract(sample_option_contract, "AAPL")

    assert result is not None
    assert result["key"] == "mkt:openbb:aapl250919c00150000"
    assert result["venue"] == "openbb"
    assert result["kind"] == "option"
    assert "CALL" in result["title"]
    assert result["venue_ref"]["underlying_symbol"] == "AAPL"
    assert result["venue_ref"]["strike"] == "150.0"


def test_normalize_option_market_empty() -> None:
    """Empty contract returns None."""
    assert normalize_market_from_option_contract({}, "AAPL") is None


# ======================================================================
# Normalizer tests — option quote
# ======================================================================


def test_normalize_option_quote(sample_option_contract: dict[str, Any]) -> None:
    """Build a canonical Quote from an option contract row."""
    result = normalize_option_quote(sample_option_contract, "AAPL")

    assert result is not None
    assert result["market"] == "mkt:openbb:aapl250919c00150000"
    assert result["bid"] == "8.50"
    assert result["ask"] == "8.65"
    assert result["mid"] == "8.575"
    assert result["last"] == "8.55"
    assert result["source"] == "poll"
    assert result["delta"] is not None
    assert result["implied_volatility"] is not None


def test_normalize_option_quote_empty() -> None:
    """Empty contract returns None."""
    assert normalize_option_quote({}, "AAPL") is None


# ======================================================================
# Adapter tests (gRPC)
# ======================================================================


@pytest.mark.asyncio
async def test_adapter_submit_order_not_implemented() -> None:
    """SubmitOrder raises UNIMPLEMENTED."""
    from connectors.venues.openbb.src.adapter import OpenbbVenueAdapter

    adapter = OpenbbVenueAdapter(client=OpenbbClient(obb=MagicMock()))

    order = MagicMock()

    context = MagicMock()
    context.abort = MagicMock(side_effect=grpc.RpcError)

    with pytest.raises(grpc.RpcError):
        adapter.SubmitOrder(order, context)

    context.abort.assert_called_once()
    assert context.abort.call_args[0][0] == grpc.StatusCode.FAILED_PRECONDITION


# ======================================================================
# Venue manifest tests
# ======================================================================


def test_venue_manifest_is_valid_toml() -> None:
    """The venue.toml is valid TOML with required fields."""
    import tomllib
    from pathlib import Path

    manifest_path = Path(__file__).resolve().parents[1] / "venue.toml"
    with open(manifest_path, "rb") as f:
        manifest = tomllib.load(f)

    assert manifest["slug"] == "openbb"
    assert manifest["display_name"] == "OpenBB"
    assert "markets" in manifest["capabilities"]
    assert "ticks" in manifest["capabilities"]
    assert "orders" not in manifest["capabilities"]
    assert manifest["asset_kinds"] == ["equity", "option"]
