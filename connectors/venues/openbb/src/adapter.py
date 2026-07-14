"""OpenBB VenueAdapter gRPC service implementation.

Implements the ``aether.venue.v1.VenueAdapter`` proto contract for equity and
options data sourced from the OpenBB platform.

Capabilities (read-only)
------------------------
- ``ListMarkets`` — stream equities from a configurable watchlist
- ``GetMarket`` — single equity or option market by key
- ``StreamTicks`` — poll quotes at configurable interval, yield as stream
- ``SubmitOrder`` / ``CancelOrder`` / ``GetBalances`` — raise
  ``FAILED_PRECONDITION`` with ``capability_missing`` (read-only pack)

AGPL isolation
--------------
OpenBB is AGPL-3.0 licensed. The OpenBB library import is confined to the
``client`` module and is never exposed outside this service process. All
inter-service communication happens through the generated gRPC stubs.
"""

from __future__ import annotations

import json
import logging
import os
import time
from datetime import datetime
from typing import Any

import grpc
from google.protobuf.timestamp_pb2 import Timestamp as ProtoTimestamp

from .client import OpenbbClient
from .normalize import (
    CanonicalMarket,
    CanonicalQuote,
    normalize_market_from_option_contract,
    normalize_market_from_profile,
    normalize_option_quote,
    normalize_quote,
)
from .proto_compile import core_pb2, market_data_pb2, venue_pb2, venue_pb2_grpc
from .quarantine import QuarantineSink

logger = logging.getLogger(__name__)

# Default symbol watchlist — overridable via env var as a comma-separated list.
_DEFAULT_WATCHLIST = ["SPY", "QQQ", "AAPL", "MSFT", "GOOGL", "AMZN", "NVDA", "TSLA"]

# Poll interval in seconds for StreamTicks.
_DEFAULT_POLL_SECS = float(
    os.environ.get("AETHER_VENUE__OPENBB_POLL_INTERVAL_SECS", "5")
)


def _resolve_watchlist() -> list[str]:
    raw = os.environ.get("AETHER_VENUE__OPENBB_WATCHLIST")
    if raw:
        return [s.strip().upper() for s in raw.split(",") if s.strip()]
    return _DEFAULT_WATCHLIST


def _parse_market_key(key_value: str) -> tuple[str, str]:
    """Parse a ``MarketKey.value`` into ``(prefix, identifier)``.

    Expected format: ``mkt:openbb:{symbol_or_occ}``.

    Returns
    -------
    tuple[str, str]
        ``(prefix, identifier)`` — e.g. ``("mkt:openbb:", "aapl")``.
    """
    prefix = "mkt:openbb:"
    if key_value.startswith(prefix):
        return prefix, key_value[len(prefix) :]
    return "", key_value


# ---------------------------------------------------------------------------
# gRPC status helpers (capability_missing)
# ---------------------------------------------------------------------------

_MISSING_CAP_ERR = """
This venue adapter is read-only. OpenBB provides market data only — orders,
cancellations, and balances are not available.
""".strip()


def _capability_missing(context: grpc.ServicerContext) -> None:
    """Abort an undeclared RPC using the SPEC-009 error contract."""
    context.abort(
        grpc.StatusCode.FAILED_PRECONDITION,
        f"capability_missing: {_MISSING_CAP_ERR}",
    )


# ---------------------------------------------------------------------------
# Servicer
# ---------------------------------------------------------------------------


class OpenbbVenueAdapter(venue_pb2_grpc.VenueAdapterServicer):
    """gRPC VenueAdapter servicer for OpenBB market data."""

    def __init__(
        self,
        client: OpenbbClient | None = None,
        watchlist: list[str] | None = None,
        poll_secs: float = _DEFAULT_POLL_SECS,
        quarantine: QuarantineSink | None = None,
    ) -> None:
        self._client = client or OpenbbClient()
        self._watchlist = watchlist or _resolve_watchlist()
        self._poll_secs = poll_secs
        self._quarantine = quarantine or QuarantineSink()
        self._last_tick_monotonic: float | None = None
        logger.info(
            "OpenBBVenueAdapter initialized (watchlist=%s, poll=%ss)",
            self._watchlist,
            self._poll_secs,
        )

    # ------------------------------------------------------------------
    # Markets
    # ------------------------------------------------------------------

    def ListMarkets(  # noqa: N802
        self,
        request: Any,
        context: Any,
    ) -> Any:
        """Stream all markets from the configured watchlist.

        Each symbol yields two entries:
        1. Equity market (from profile data)
        2. One entry per active option contract (from options chain)

        The ``filter`` field on the request can be a comma-separated list of
        symbols to scope the response (instead of the full watchlist).
        """
        symbols = self._resolve_list_symbols(request.filter)

        for symbol in symbols:
            # -- Equity market --
            profile = self._client.get_reference(symbol)
            try:
                mkt = normalize_market_from_profile(symbol, profile)
            except (TypeError, ValueError) as exc:
                self._quarantine.preserve(str(exc), profile)
                logger.warning("quarantined malformed OpenBB profile for %s", symbol)
                continue
            if mkt is not None:
                yield self._market_to_proto(mkt)

            # -- Option contracts --
            try:
                chain = self._client.get_options_chain(symbol)
            except Exception:
                logger.warning(
                    "ListMarkets: options chain for %s failed, skipping",
                    symbol,
                )
                continue

            for contract in chain:
                try:
                    opt_mkt = normalize_market_from_option_contract(contract, symbol)
                except (TypeError, ValueError) as exc:
                    self._quarantine.preserve(str(exc), contract)
                    logger.warning(
                        "quarantined malformed option contract for %s", symbol
                    )
                    continue
                if opt_mkt is not None:
                    yield self._market_to_proto(opt_mkt)

    def GetMarket(  # noqa: N802
        self,
        request: Any,
        context: Any,
    ) -> Any:
        """Get a single market by key.

        Supports both equity (``mkt:openbb:aapl``) and option
        (``mkt:openbb:{occ_symbol}``) market keys.
        """
        key_value = request.key.value
        prefix, ident = _parse_market_key(key_value)

        if prefix != "mkt:openbb:" or not ident or ":" in ident:
            context.abort(
                grpc.StatusCode.INVALID_ARGUMENT,
                f"invalid market key: {key_value!r}",
            )

        # Try as an equity symbol first
        symbol = ident.upper()
        profile = self._client.get_reference(symbol)
        mkt = normalize_market_from_profile(symbol, profile)

        if mkt is not None:
            return self._market_to_proto(mkt)

        # Not found via profile — try options chain lookup
        # (worst case: scan the watchlist for an underlying)
        for candidate in self._watchlist:
            chain = self._client.get_options_chain(candidate)
            occ_symbols = {
                c.get("contract_symbol", "").lower()
                for c in chain
                if c.get("contract_symbol")
            }
            if ident.lower() in occ_symbols:
                # Find the matching contract
                for c in chain:
                    if c.get("contract_symbol", "").lower() == ident.lower():
                        opt_mkt = normalize_market_from_option_contract(c, candidate)
                        if opt_mkt is not None:
                            return self._market_to_proto(opt_mkt)
                        break

        context.abort(
            grpc.StatusCode.NOT_FOUND,
            f"market not found: {key_value!r}",
        )

    # ------------------------------------------------------------------
    # Ticks (polling-based)
    # ------------------------------------------------------------------

    def StreamTicks(  # noqa: N802
        self,
        request: Any,
        context: Any,
    ) -> Any:
        """Stream quotes by polling OpenBB at a configurable interval.

        Quotes are read-only snapshots from the REST API source, so the
        ``source`` field is set to ``QUOTE_SOURCE_POLL``.

        The ``keys`` repeated field specifies which markets to poll. If
        empty, the configured watchlist is used.
        """
        tick_keys: list[str] = []
        if request.keys:
            tick_keys = [k.value for k in request.keys]

        if not tick_keys:
            tick_keys = [f"mkt:openbb:{s.lower()}" for s in self._watchlist]

        logger.debug(
            "StreamTicks: polling %d keys every %.1fs",
            len(tick_keys),
            self._poll_secs,
        )

        while context.is_active():
            # Collect symbol-level quotes from the watchlist
            parsed = [_parse_market_key(key) for key in tick_keys]
            invalid = [
                key
                for key, (prefix, ident) in zip(tick_keys, parsed, strict=True)
                if prefix != "mkt:openbb:" or not ident or ":" in ident
            ]
            if invalid:
                context.abort(
                    grpc.StatusCode.INVALID_ARGUMENT,
                    f"invalid market key: {invalid[0]!r}",
                )
            identifiers = [ident for _, ident in parsed]
            equity_symbols = [
                ident.upper()
                for ident in identifiers
                if ident.upper() in self._watchlist
            ]

            if equity_symbols:
                quotes_map = self._batch_quotes(equity_symbols)
                for _mkey, q in quotes_map.items():
                    self._last_tick_monotonic = time.monotonic()
                    yield self._quote_to_proto(q)

            option_ids = {
                ident.lower()
                for ident in identifiers
                if ident.upper() not in self._watchlist
            }
            for underlying in self._watchlist:
                if not option_ids:
                    break
                for contract in self._client.get_options_chain(underlying):
                    occ = str(contract.get("contract_symbol", "")).lower()
                    if occ in option_ids:
                        quote = normalize_option_quote(contract, underlying)
                        if quote is not None:
                            self._last_tick_monotonic = time.monotonic()
                            yield self._quote_to_proto(quote)
                        option_ids.remove(occ)

            # Sleep until next poll cycle
            time.sleep(self._poll_secs)

    # ------------------------------------------------------------------
    # Unsupported operations (read-only pack)
    # ------------------------------------------------------------------

    def SubmitOrder(  # noqa: N802
        self,
        request: Any,
        context: Any,
    ) -> Any:
        """Not implemented — OpenBB is a read-only data source."""
        _capability_missing(context)

    def CancelOrder(  # noqa: N802
        self,
        request: Any,
        context: Any,
    ) -> Any:
        """Not implemented — OpenBB is a read-only data source."""
        _capability_missing(context)

    def GetBalances(  # noqa: N802
        self,
        request: Any,
        context: Any,
    ) -> Any:
        """Not implemented — OpenBB is a read-only data source."""
        _capability_missing(context)

    def StreamBook(  # noqa: N802
        self,
        request: Any,
        context: Any,
    ) -> Any:
        """Not implemented — OpenBB has no order book streaming."""
        _capability_missing(context)

    # ------------------------------------------------------------------
    # Health
    # ------------------------------------------------------------------

    def Health(  # noqa: N802
        self,
        request: Any,
        context: Any,
    ) -> Any:
        """Return a VenueHealth response."""
        healthy = self._client.check_connectivity()
        lag_ms = self._feed_lag_ms()
        ready = healthy and lag_ms <= int(self._poll_secs * 2_000)
        return venue_pb2.VenueHealth(
            status="ok" if ready else "degraded",
            lag_ms=lag_ms,
            rate_remaining=self._client.rate_remaining,
        )

    def is_ready(self) -> bool:
        """Readiness requires provider connectivity and a recent normalized tick."""
        return self._client.check_connectivity() and self._feed_lag_ms() <= int(
            self._poll_secs * 2_000
        )

    def _feed_lag_ms(self) -> int:
        if self._last_tick_monotonic is None:
            return 2**64 - 1
        return max(0, int((time.monotonic() - self._last_tick_monotonic) * 1_000))

    # ------------------------------------------------------------------
    # Internal helpers
    # ------------------------------------------------------------------

    def _resolve_list_symbols(self, filter_str: str | None) -> list[str]:
        """Resolve the list of symbols for ListMarkets.

        If ``filter_str`` is a non-empty comma-separated list, use that.
        Otherwise fall back to the configured watchlist.
        """
        if filter_str and filter_str.strip():
            return [s.strip().upper() for s in filter_str.split(",") if s.strip()]
        return self._watchlist

    def _batch_quotes(self, symbols: list[str]) -> dict[str, CanonicalQuote]:
        """Fetch and normalise quotes for a list of equity symbols.

        Returns a dict mapping market key to canonical quote.
        """
        raw_quotes = self._client.get_quotes(symbols)
        result: dict[str, Any] = {}
        requested = {symbol.upper() for symbol in symbols}
        for raw in raw_quotes:
            sym = str(raw.get("symbol", "")).upper()
            if not sym or sym not in requested:
                logger.warning(
                    "discarding quote with missing/unrequested symbol: %r", sym
                )
                continue
            try:
                q = normalize_quote(raw, sym)
            except (TypeError, ValueError) as exc:
                self._quarantine.preserve(str(exc), raw)
                logger.warning("quarantined malformed quote for %s", sym)
                continue
            if q is not None:
                result[q["market"]] = q
        return result

    # ------------------------------------------------------------------
    # Proto builders
    # ------------------------------------------------------------------

    def _market_to_proto(self, mkt: CanonicalMarket) -> Any:
        """Convert a canonical market dict to a proto ``Market`` message."""
        return market_data_pb2.Market(
            key=core_pb2.MarketKey(value=mkt["key"]),
            venue=core_pb2.VenueId(value=mkt["venue"]),
            kind=self._instrument_kind(mkt["kind"]),
            title=mkt["title"],
            description_ref="",
            status=self._market_status(mkt["status"]),
            close_ts=self._proto_timestamp(mkt.get("close_ts")),
            resolve_ts=self._proto_timestamp(mkt.get("resolve_ts")),
            outcome=mkt.get("outcome") or "",
            jurisdiction_flags=mkt.get("jurisdiction_flags", []),
            venue_ref=json.dumps(mkt.get("venue_ref", {})),
            meta=json.dumps(mkt.get("meta", {})),
        )

    def _quote_to_proto(self, q: CanonicalQuote) -> Any:
        """Convert a canonical quote dict to a proto ``Quote`` message."""
        return market_data_pb2.Quote(
            market=core_pb2.MarketKey(value=q["market"]),
            bid=q.get("bid") or "",
            ask=q.get("ask") or "",
            mid=q.get("mid") or "",
            last=q.get("last") or "",
            bid_size=q.get("bid_size") or "",
            ask_size=q.get("ask_size") or "",
            source=market_data_pb2.QUOTE_SOURCE_POLL,
            ts=self._proto_timestamp(q.get("ts")),
            seq=0,
        )

    @staticmethod
    def _proto_timestamp(ts: str | None) -> ProtoTimestamp | None:
        """Convert an ISO-8601 string to a ``google.protobuf.Timestamp``.

        Returns ``None`` if the timestamp is ``None``, empty, or unparsable.
        """
        if not ts:
            return None
        try:
            dt = datetime.fromisoformat(ts.replace("Z", "+00:00"))
            t = ProtoTimestamp()
            t.FromDatetime(dt)
            return t
        except (ValueError, TypeError):
            return None

    @staticmethod
    def _instrument_kind(kind: str) -> int:
        """Map a canonical kind string to proto ``InstrumentKind`` enum value."""
        mapping = {
            "equity": market_data_pb2.INSTRUMENT_KIND_EQUITY,
            "option": market_data_pb2.INSTRUMENT_KIND_OPTION,
            "binary_contract": market_data_pb2.INSTRUMENT_KIND_BINARY_CONTRACT,
        }
        return mapping.get(kind, market_data_pb2.INSTRUMENT_KIND_UNSPECIFIED)

    @staticmethod
    def _market_status(status: str) -> int:
        """Map a status string to proto ``MarketStatus`` enum value."""
        mapping = {
            "open": market_data_pb2.MARKET_STATUS_OPEN,
            "closed": market_data_pb2.MARKET_STATUS_CLOSED,
            "halted": market_data_pb2.MARKET_STATUS_HALTED,
            "settled": market_data_pb2.MARKET_STATUS_RESOLVED,
            "resolved": market_data_pb2.MARKET_STATUS_RESOLVED,
        }
        return mapping.get(status.lower(), market_data_pb2.MARKET_STATUS_UNSPECIFIED)
