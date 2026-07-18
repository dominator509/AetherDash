/**
 * Real-time order book display (SPEC-004 Surface 4).
 *
 * Shows bids (descending), asks (ascending), spread, and mid-price.
 * Updates via WebSocket subscription to md.books.{venue}.
 *
 * Features:
 * - Depth bars at each level (bid=green, ask=red)
 * - Cumulative total column
 * - Spread and midpoint indicator
 * - Configurable precision and max levels
 * - Last-update timestamp
 */

import { useMemo } from "react";

// ── Types ──────────────────────────────────────────────────────────────────────

export interface BookLevel {
  price: string; // Decimal string
  size: string; // Decimal string
  total: string; // Cumulative size
}

export interface OrderBookData {
  market: string;
  bids: BookLevel[];
  asks: BookLevel[];
  spread: string;
  midPrice: string | null;
  lastUpdate: number;
  depth: number;
}

export interface OrderBookProps {
  data: OrderBookData;
  maxLevels?: number;
  precision?: number;
}

// ── Constants ──────────────────────────────────────────────────────────────────

const DEFAULT_MAX_LEVELS = 20;
const DEFAULT_PRECISION = 4;

// ── Component ──────────────────────────────────────────────────────────────────

export function OrderBook({
  data,
  maxLevels = DEFAULT_MAX_LEVELS,
  precision = DEFAULT_PRECISION,
}: OrderBookProps) {
  // Build display bids: reverse so highest-price bid is first (descending order),
  // with cumulative total computed per-visibility row.
  const displayBids = useMemo(() => {
    const visible = data.bids.slice(0, maxLevels);
    const withTotal = visible.map((level, i, arr) => ({
      ...level,
      total: arr
        .slice(0, i + 1)
        .reduce((sum, l) => sum + parseFloat(l.size), 0)
        .toFixed(precision),
    }));
    return withTotal.reverse();
  }, [data.bids, maxLevels, precision]);

  // Build display asks: lowest-price ask first (ascending order),
  // with cumulative total.
  const displayAsks = useMemo(() => {
    const visible = data.asks.slice(0, maxLevels);
    return visible.map((level, i, arr) => ({
      ...level,
      total: arr
        .slice(0, i + 1)
        .reduce((sum, l) => sum + parseFloat(l.size), 0)
        .toFixed(precision),
    }));
  }, [data.asks, maxLevels, precision]);

  // Max total for depth-bar scaling
  const maxTotal = useMemo(() => {
    const allTotals = [...displayBids, ...displayAsks].map((l) => parseFloat(l.total));
    return Math.max(...allTotals, 1);
  }, [displayBids, displayAsks]);

  // Spread as percentage of best bid
  const spreadPct = useMemo(() => {
    if (data.bids.length === 0 || data.asks.length === 0) return null;
    const bestBid = data.bids[0] ? parseFloat(data.bids[0].price) : 0;
    const bestAsk = data.asks[0] ? parseFloat(data.asks[0].price) : 0;
    if (bestBid <= 0) return null;
    return (((bestAsk - bestBid) / bestBid) * 100).toFixed(3);
  }, [data.bids, data.asks]);

  const hasData = data.bids.length > 0 || data.asks.length > 0;

  return (
    <div className="flex flex-col h-full font-mono text-xs" role="region" aria-label="Order book">
      {/* Spread indicator */}
      <div className="flex items-center justify-between px-2 py-1 bg-gray-50 border-b border-gray-200">
        <span className="text-gray-500" aria-label="Spread">
          Spread: {spreadPct !== null ? `${spreadPct}%` : "N/A"}
        </span>
        {data.midPrice && (
          <span className="font-bold text-blue-600" aria-label="Mid price">
            Mid: {data.midPrice}
          </span>
        )}
      </div>

      {/* Column headers */}
      <div className="flex px-2 py-1 text-gray-400 border-b border-gray-200 text-[10px] uppercase tracking-wider">
        <span className="w-1/3 text-right">Price</span>
        <span className="w-1/3 text-right">Size</span>
        <span className="w-1/3 text-right">Total</span>
      </div>

      {!hasData && (
        <div className="flex flex-1 items-center justify-center text-gray-400 text-xs">
          Awaiting market data&hellip;
        </div>
      )}

      {hasData && (
        <>
          {/* Asks (sells) — lowest at bottom, deepest at top */}
          <div className="flex-1 overflow-hidden" role="list" aria-label="Asks">
            {displayAsks.map((level, i) => (
              <div
                key={`ask-${i}`}
                role="listitem"
                className="flex px-2 py-[1px] relative hover:bg-gray-50"
              >
                <div
                  className="absolute right-0 top-0 bottom-0 bg-red-100 opacity-30 pointer-events-none"
                  style={{
                    width: `${(parseFloat(level.total) / maxTotal) * 100}%`,
                  }}
                />
                <span className="w-1/3 text-right text-red-600 z-10">
                  {parseFloat(level.price).toFixed(precision)}
                </span>
                <span className="w-1/3 text-right z-10">
                  {parseFloat(level.size).toFixed(precision)}
                </span>
                <span className="w-1/3 text-right text-gray-400 z-10">{level.total}</span>
              </div>
            ))}
          </div>

          {/* Bids (buys) — highest at top, deepest at bottom */}
          <div className="flex-1 overflow-hidden" role="list" aria-label="Bids">
            {displayBids.map((level, i) => (
              <div
                key={`bid-${i}`}
                role="listitem"
                className="flex px-2 py-[1px] relative hover:bg-gray-50"
              >
                <div
                  className="absolute right-0 top-0 bottom-0 bg-green-100 opacity-30 pointer-events-none"
                  style={{
                    width: `${(parseFloat(level.total) / maxTotal) * 100}%`,
                  }}
                />
                <span className="w-1/3 text-right text-green-600 z-10">
                  {parseFloat(level.price).toFixed(precision)}
                </span>
                <span className="w-1/3 text-right z-10">
                  {parseFloat(level.size).toFixed(precision)}
                </span>
                <span className="w-1/3 text-right text-gray-400 z-10">{level.total}</span>
              </div>
            ))}
          </div>
        </>
      )}

      {/* Timestamp footer */}
      <div className="px-2 py-1 text-gray-400 text-right border-t border-gray-200 text-[10px]">
        {data.lastUpdate > 0 ? new Date(data.lastUpdate).toLocaleTimeString() : "No data"}
      </div>
    </div>
  );
}
