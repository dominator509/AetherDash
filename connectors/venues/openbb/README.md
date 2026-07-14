# AETHER Terminal -- OpenBB Venue Adapter

**Python gRPC service** providing equity/options quotes and reference data
via the [OpenBB Platform](https://openbb.co) library.

## AGPL-3.0 License Notice

**OpenBB** is licensed under the **GNU Affero General Public License v3.0
(AGPL-3.0)**.  To maintain the AETHER Terminal's license boundary (D7), this
venue adapter runs as a **separate gRPC service process**.  The OpenBB Python
library is imported exclusively within this pack; no OpenBB code crosses the
network boundary into the broader AETHER codebase.

This adapter itself is released under AGPL-3.0 to match OpenBB's copyleft
terms at the service boundary.

See `aether-blueprint/DECISIONS.md` for the AGPL isolation decision record.

## Capabilities

| Feature | Status |
|---------|--------|
| Equity quotes | Yes (polling, configurable interval) |
| Options chains | Yes (full chain per symbol) |
| Company reference data | Yes |
| Market listing | Yes (watchlist-driven) |
| Order submission | No (read-only pack) |
| Order cancellation | No |
| Balance queries | No |
| Order book streaming | No (OpenBB REST-based) |

## Quick Start

```bash
# From the repository root
# Resolve the repository lock and run from the workspace root
uv sync

# Run the gRPC server
uv run python -m connectors.venues.openbb.src.server
```

## Configuration

| Environment Variable | Default | Description |
|----------------------|---------|-------------|
| `AETHER_VENUE__OPENBB_GRPC_ADDR` | `127.0.0.1:50058` | gRPC bind address |
| `AETHER_VENUE__OPENBB_HEALTH_PORT` | `8088` | HTTP health check port |
| `AETHER_VENUE__OPENBB_PROVIDER` | `yfinance` | OpenBB data provider |
| `AETHER_VENUE__OPENBB_POLL_INTERVAL_SECS` | `5` | Tick poll interval |
| `AETHER_VENUE__OPENBB_WATCHLIST` | *built-in* | Comma-separated symbols |
| `AETHER_VENUE__OPENBB_*` | — | Provider-specific API keys (secret) |

## gRPC Contract

Implements `aether.venue.v1.VenueAdapter`:

- `ListMarkets` — streams equity + option markets from the watchlist
- `GetMarket` — single equity or option market by MarketKey
- `StreamTicks` — polls quotes at interval, yields as Quote stream
- `SubmitOrder`, `CancelOrder`, `GetBalances`, `StreamBook` → `FAILED_PRECONDITION` (`capability_missing`)

## Market Key Format

- Equity: `mkt:openbb:{symbol}` (e.g. `mkt:openbb:aapl`)
- Option: `mkt:openbb:{occ_symbol}` (e.g. `mkt:openbb:aapl250919c00150000`)

## Architecture

```
┌───────────────────────────────────────────┐
│  AETHER System                            │
│  ┌─────────────────────────────────────┐  │
│  │ Order Router (gRPC client)          │  │
│  └────────┬────────────────────────────┘  │
│           │ gRPC (network boundary)        │
│  ┌────────▼────────────────────────────┐  │
│  │ OpenBB Venue Adapter (this pack)    │  │
│  │  - proto_compile.py (proto→stubs)   │  │
│  │  - adapter.py (VenueAdapter impl)   │  │
│  │  - client.py (OpenBB wrapper)       │  │
│  │  - normalize.py (market normalize)  │  │
│  │  - health.py (HTTP health endpoint) │  │
│  └────────┬────────────────────────────┘  │
│           │ Python import (isolated)       │
│  ┌────────▼────────────────────────────┐  │
│  │ OpenBB Platform (AGPL-3.0)          │  │
│  │  - openbb-yfinance (free provider)  │  │
│  └─────────────────────────────────────┘  │
└───────────────────────────────────────────┘
```

## Testing

```bash
# Unit tests (mocked OpenBB)
pytest connectors/venues/openbb/tests/

# With coverage
pytest --cov=connectors.venues.openbb connectors/venues/openbb/tests/
```
