"""Market normalisation: OpenBB raw data -> canonical aether-core shapes.

Mapping rules (SPEC-009)
------------------------
- **MarketKey** (equity) :math:`\\rightarrow` ``mkt:openbb:{symbol}`` (lowercased)
- **MarketKey** (option) :math:`\\rightarrow` ``mkt:openbb:{occ_symbol}`` (lowercased)
- **InstrumentKind** ``equity`` for common stock, ``option`` for option contracts
- **Prices** represented as decimal strings (SPEC-001)
- **Timestamps** as ISO-8601 / ``google.protobuf.Timestamp``
"""

from __future__ import annotations

from datetime import UTC, datetime
from decimal import Decimal, DecimalException
from typing import Any

# ---------------------------------------------------------------------------
# Canonical shape type aliases (plain dicts -- no proto dependency at this
# level so the normalizer remains testable without gRPC).
# ---------------------------------------------------------------------------

CanonicalQuote = dict[str, Any]
"""Keys: market, bid, ask, mid, last, bid_size, ask_size, ts, source."""

CanonicalMarket = dict[str, Any]
"""Keys: key, venue, kind, title, status, venue_ref, meta."""

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

SLUG = "openbb"
VENUE_PREFIX = f"mkt:{SLUG}:"


# ---------------------------------------------------------------------------
# Normalisers
# ---------------------------------------------------------------------------


def normalize_quote(raw: dict[str, Any], symbol: str) -> CanonicalQuote | None:
    """Convert an OpenBB quote response to a canonical Quote dict.

    Parameters
    ----------
    raw : dict
        Single quote result from the OpenBB ``equity.price.quote`` endpoint.
    symbol : str
        The equity symbol (used to build the market key).

    Returns
    -------
    CanonicalQuote or None
        None if the input is empty or lacks a last price.
    """
    if not raw:
        return None

    bid = _to_decimal(raw.get("bid"))
    ask = _to_decimal(raw.get("ask"))
    last = _to_decimal(raw.get("last_price") or raw.get("last"))

    # Compute mid from bid/ask if both present
    mid: str | None = None
    if bid is not None and ask is not None:
        mid_dec = (Decimal(bid) + Decimal(ask)) / Decimal("2")
        mid = str(mid_dec)

    ts = _parse_timestamp(raw.get("last_trade_time") or raw.get("timestamp"))

    return {
        "market": f"{VENUE_PREFIX}{symbol.lower()}",
        "bid": bid,
        "ask": ask,
        "mid": mid,
        "last": last,
        "bid_size": _to_decimal(raw.get("bid_size")),
        "ask_size": _to_decimal(raw.get("ask_size")),
        "ts": ts,
        "source": "poll",
        "seq": None,
    }


def normalize_market_from_profile(
    symbol: str,
    profile: dict[str, Any],
) -> CanonicalMarket | None:
    """Build a canonical Market from OpenBB profile/reference data.

    Parameters
    ----------
    symbol : str
        Equity ticker symbol.
    profile : dict
        Profile data from ``obb.equity.profile()``.

    Returns
    -------
    CanonicalMarket or None
    """
    if not profile:
        return None

    return {
        "key": f"{VENUE_PREFIX}{symbol.lower()}",
        "venue": SLUG,
        "kind": "equity",
        "title": profile.get("name") or profile.get("symbol", symbol),
        "status": "open",  # OpenBB doesn't provide trading status in profile
        "close_ts": None,
        "resolve_ts": None,
        "outcome": None,
        "jurisdiction_flags": ["US"],
        "venue_ref": {
            "symbol": symbol,
            "name": profile.get("name"),
            "exchange": profile.get("exchange"),
            "sector": profile.get("sector"),
            "industry": profile.get("industry"),
            "market_cap": profile.get("market_cap"),
            "currency": profile.get("currency", "USD"),
        },
        "meta": {
            "provider": "openbb",
            "instrument_type": "equity",
        },
    }


def normalize_market_from_option_contract(
    contract: dict[str, Any],
    underlying_symbol: str,
) -> CanonicalMarket | None:
    """Build a canonical Market from an OpenBB options chain contract row.

    Parameters
    ----------
    contract : dict
        Single contract row from ``obb.derivatives.options.chains()``.
    underlying_symbol : str
        The underlying equity symbol.

    Returns
    -------
    CanonicalMarket or None
    """
    if not contract:
        return None

    occ_symbol = contract.get("contract_symbol", "")
    if not occ_symbol:
        return None

    option_type = contract.get("option_type", "").lower()
    strike = str(contract.get("strike", ""))

    # Build a human-readable title
    exp = contract.get("expiration", "")
    title = (
        f"{underlying_symbol} {exp} {option_type.upper()} {strike} [occ: {occ_symbol}]"
    )

    return {
        "key": f"{VENUE_PREFIX}{occ_symbol.lower()}",
        "venue": SLUG,
        "kind": "option",
        "title": title,
        "status": "open",
        "close_ts": _parse_timestamp(contract.get("expiration")),
        "resolve_ts": None,
        "outcome": None,
        "jurisdiction_flags": ["US"],
        "venue_ref": {
            "contract_symbol": occ_symbol,
            "underlying_symbol": underlying_symbol,
            "expiration": exp,
            "strike": strike,
            "option_type": option_type,
            "contract_size": contract.get("contract_size"),
        },
        "meta": {
            "provider": "openbb",
            "instrument_type": "option",
        },
    }


def normalize_option_quote(
    contract: dict[str, Any],
    underlying_symbol: str,
) -> CanonicalQuote | None:
    """Convert a single options-chain contract row to a canonical Quote.

    Parameters
    ----------
    contract : dict
        Single contract row from the options chain response.
    underlying_symbol : str
        The underlying equity symbol.

    Returns
    -------
    CanonicalQuote or None
    """
    if not contract:
        return None

    occ_symbol = contract.get("contract_symbol", "")
    if not occ_symbol:
        return None

    bid = _to_decimal(contract.get("bid"))
    ask = _to_decimal(contract.get("ask"))
    last = _to_decimal(contract.get("last_trade_price") or contract.get("last"))

    mid: str | None = None
    if bid is not None and ask is not None:
        mid_dec = (Decimal(bid) + Decimal(ask)) / Decimal("2")
        mid = str(mid_dec)

    # Greeks and theoretical price go into meta
    meta = {}
    for greek in ("delta", "gamma", "theta", "vega", "rho", "implied_volatility"):
        val = contract.get(greek)
        if val is not None:
            meta[greek] = str(val)

    return {
        "market": f"{VENUE_PREFIX}{occ_symbol.lower()}",
        "bid": bid,
        "ask": ask,
        "mid": mid,
        "last": last,
        "bid_size": _to_decimal(contract.get("bid_size")),
        "ask_size": _to_decimal(contract.get("ask_size")),
        "ts": _parse_timestamp(contract.get("last_trade_time")),
        "source": "poll",
        "seq": None,
        # Extra option-specific metadata
        "open_interest": contract.get("open_interest"),
        "volume": contract.get("volume"),
        "implied_volatility": _to_decimal(contract.get("implied_volatility")),
        "delta": _to_decimal(contract.get("delta")),
        "gamma": _to_decimal(contract.get("gamma")),
        "theta": _to_decimal(contract.get("theta")),
        "vega": _to_decimal(contract.get("vega")),
    }


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _to_decimal(value: Any) -> str | None:
    """Convert a venue value to exact decimal text.

    Malformed numeric fields are rejected so callers can quarantine the raw
    payload instead of silently converting bad data into a missing field.
    """
    if value is None:
        return None
    try:
        return str(Decimal(str(value)))
    except (DecimalException, ValueError, TypeError) as exc:
        raise ValueError(f"invalid decimal value {value!r}") from exc


def _parse_timestamp(value: Any) -> str | None:
    """Parse a timestamp value into an ISO-8601 string.

    Accepts ISO strings, Unix millis (int), or ``datetime`` objects.
    Returns ``None`` if parsing fails.
    """
    if value is None:
        return None

    # Already a string
    if isinstance(value, str):
        try:
            dt = datetime.fromisoformat(value.replace("Z", "+00:00"))
            return dt.isoformat()
        except (ValueError, TypeError) as exc:
            raise ValueError(f"invalid timestamp {value!r}") from exc

    # Numeric (Unix millis / seconds)
    if isinstance(value, (int, float)):
        try:
            # Assume seconds if < 1e12, millis otherwise
            if value > 1e12:
                value = value / 1000.0
            dt = datetime.fromtimestamp(float(value), tz=UTC)
            return dt.isoformat()
        except (ValueError, OSError) as exc:
            raise ValueError(f"invalid timestamp {value!r}") from exc

    # datetime
    if isinstance(value, datetime):
        return value.isoformat()

    raise ValueError(f"unsupported timestamp value {value!r}")
