/**
 * Depth of Market (DOM) surface (SPEC-004 Surface 4).
 *
 * Full-featured order book visualization with:
 * - Real-time bids/asks with depth bars
 * - Market selector (venue + market pair)
 * - Configurable price precision
 * - Toggleable columns (price, size, total)
 * - Spread and mid-price indicators
 * - Configurable depth levels
 * - Auto-refresh or manual snapshot mode
 */

import React, { useState, useCallback, useRef, useEffect } from "react";
import { OrderBook, type OrderBookData } from "@/components/order-book";

// ── Types ──────────────────────────────────────────────────────────────────────

export interface MarketOption {
  id: string;
  label: string;
  venue: string;
}

export type DomColumn = "price" | "size" | "total";

export interface DepthOfMarketProps {
  /** Available markets to select from. */
  markets?: MarketOption[];
  /** Currently subscribed market data (or null while loading). */
  data?: OrderBookData | null;
  /** Callback when market selection changes. */
  onMarketChange?: (marketId: string) => void;
  /** Callback when precision changes. */
  onPrecisionChange?: (precision: number) => void;
}

// ── Default markets ────────────────────────────────────────────────────────────

export const DEFAULT_MARKETS: MarketOption[] = [
  { id: "kalshi:USDX", label: "USD Index (USDX)", venue: "Kalshi" },
  { id: "kalshi:SPX", label: "S&P 500 (SPX)", venue: "Kalshi" },
  { id: "kalshi:10Y", label: "10Y Treasury Yield", venue: "Kalshi" },
  { id: "poly:ETH-USD", label: "ETH / USD", venue: "Polymarket" },
  { id: "poly:BTC-USD", label: "BTC / USD", venue: "Polymarket" },
  { id: "poly:MATIC-USD", label: "MATIC / USD", venue: "Polymarket" },
];

// ── Helpers ────────────────────────────────────────────────────────────────────

function generateMockLevel(
  basePrice: number,
  index: number,
  side: "bid" | "ask",
  precision: number,
): { price: string; size: string } {
  const direction = side === "ask" ? 1 : -1;
  const offset = (index + 1) * 0.01 * direction;
  const jitter = (Math.random() - 0.5) * 0.005;
  const price = basePrice + offset + jitter;
  const size = Math.random() * 500 + 10;
  return {
    price: price.toFixed(precision),
    size: size.toFixed(precision),
  };
}

function buildMockOrderBook(
  basePrice: number,
  levels: number,
  precision: number,
): OrderBookData {
  const bids = [];
  const asks = [];

  for (let i = 0; i < levels; i++) {
    const bid = generateMockLevel(basePrice, i, "bid", precision);
    const ask = generateMockLevel(basePrice, i, "ask", precision);
    bids.push({ ...bid, total: "0" });
    asks.push({ ...ask, total: "0" });
  }

  // Best bid is first in array (highest bid)
  bids.sort(
    (a, b) => parseFloat(b.price) - parseFloat(a.price),
  );
  // Best ask is first in array (lowest ask)
  asks.sort(
    (a, b) => parseFloat(a.price) - parseFloat(b.price),
  );

  const bestBid = parseFloat(bids[0]?.price ?? "0");
  const bestAsk = parseFloat(asks[0]?.price ?? "0");

  return {
    market: "demo",
    bids,
    asks,
    spread: bestBid > 0 && bestAsk > 0
      ? (bestAsk - bestBid).toFixed(precision)
      : "0",
    midPrice:
      bestBid > 0 && bestAsk > 0
        ? ((bestBid + bestAsk) / 2).toFixed(precision)
        : null,
    lastUpdate: Date.now(),
    depth: levels,
  };
}

// ── Precision presets ──────────────────────────────────────────────────────────

const PRECISION_OPTIONS = [
  { value: 2, label: "0.01" },
  { value: 3, label: "0.001" },
  { value: 4, label: "0.0001" },
  { value: 5, label: "0.00001" },
];

const DEPTH_OPTIONS = [
  { value: 10, label: "10" },
  { value: 20, label: "20" },
  { value: 50, label: "50" },
  { value: 100, label: "100" },
];

// ── Component ──────────────────────────────────────────────────────────────────

export function DepthOfMarket({
  markets = DEFAULT_MARKETS,
  data: externalData,
  onMarketChange,
  onPrecisionChange,
}: DepthOfMarketProps) {
  const [selectedMarket, setSelectedMarket] = useState(markets[0]?.id ?? "");
  const [precision, setPrecision] = useState(4);
  const [maxLevels, setMaxLevels] = useState(20);
  const [visibleColumns, setVisibleColumns] = useState<Set<DomColumn>>(
    new Set(["price", "size", "total"]),
  );
  const [autoRefresh, setAutoRefresh] = useState(true);
  const [internalData, setInternalData] = useState<OrderBookData | null>(null);

  const refreshTimerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // Use external data if provided, otherwise generate mock data for development
  const displayData = externalData ?? internalData;

  // Auto-refresh mock data every 1.5s for dev mode
  useEffect(() => {
    if (!autoRefresh) {
      if (refreshTimerRef.current) {
        clearInterval(refreshTimerRef.current);
        refreshTimerRef.current = null;
      }
      return;
    }

    const basePrice = selectedMarket.includes("BTC") ? 65420 : selectedMarket.includes("ETH") ? 3420 : 100.5;
    const updateMock = () => {
      setInternalData(buildMockOrderBook(basePrice, maxLevels, precision));
    };

    updateMock();
    refreshTimerRef.current = setInterval(updateMock, 1500);

    return () => {
      if (refreshTimerRef.current) {
        clearInterval(refreshTimerRef.current);
        refreshTimerRef.current = null;
      }
    };
  }, [autoRefresh, selectedMarket, precision, maxLevels]);

  // Toggle column visibility
  const toggleColumn = useCallback((col: DomColumn) => {
    setVisibleColumns((prev) => {
      const next = new Set(prev);
      // Keep at least one column visible
      if (next.size <= 1 && next.has(col)) return prev;
      if (next.has(col)) {
        next.delete(col);
      } else {
        next.add(col);
      }
      return next;
    });
  }, []);

  // Handle market change
  const handleMarketChange = useCallback(
    (e: React.ChangeEvent<HTMLSelectElement>) => {
      const id = e.target.value;
      setSelectedMarket(id);
      onMarketChange?.(id);
    },
    [onMarketChange],
  );

  // Handle precision change
  const handlePrecisionChange = useCallback(
    (e: React.ChangeEvent<HTMLSelectElement>) => {
      const p = parseInt(e.target.value, 10);
      setPrecision(p);
      onPrecisionChange?.(p);
    },
    [onPrecisionChange],
  );

  // Handle depth change
  const handleDepthChange = useCallback(
    (e: React.ChangeEvent<HTMLSelectElement>) => {
      setMaxLevels(parseInt(e.target.value, 10));
    },
    [],
  );

  // Force manual refresh
  const handleRefresh = useCallback(() => {
    if (!selectedMarket) return;
    // When external data is provided, refresh is delegated upward
    if (externalData) {
      onMarketChange?.(selectedMarket);
      return;
    }
    // Otherwise regenerate mock data
    const basePrice = selectedMarket.includes("BTC") ? 65420 : selectedMarket.includes("ETH") ? 3420 : 100.5;
    setInternalData(buildMockOrderBook(basePrice, maxLevels, precision));
  }, [selectedMarket, externalData, onMarketChange, maxLevels, precision]);

  const selectedMarketLabel =
    markets.find((m) => m.id === selectedMarket)?.label ?? selectedMarket;

  return (
    <div className="flex flex-col h-full bg-white" data-surface="depth-of-market">
      {/* Toolbar */}
      <div className="flex items-center gap-2 px-2 py-1.5 border-b border-gray-200 bg-gray-50 flex-shrink-0">
        {/* Market selector */}
        <select
          className="text-xs border border-gray-300 rounded px-1.5 py-0.5 bg-white max-w-[160px]"
          value={selectedMarket}
          onChange={handleMarketChange}
          aria-label="Select market"
        >
          {markets.map((m) => (
            <option key={m.id} value={m.id}>
              {m.label}
            </option>
          ))}
        </select>

        {/* Precision selector */}
        <select
          className="text-xs border border-gray-300 rounded px-1.5 py-0.5 bg-white"
          value={precision}
          onChange={handlePrecisionChange}
          aria-label="Price precision"
        >
          {PRECISION_OPTIONS.map((opt) => (
            <option key={opt.value} value={opt.value}>
              {opt.label}
            </option>
          ))}
        </select>

        {/* Depth selector */}
        <select
          className="text-xs border border-gray-300 rounded px-1.5 py-0.5 bg-white"
          value={maxLevels}
          onChange={handleDepthChange}
          aria-label="Depth levels"
        >
          {DEPTH_OPTIONS.map((opt) => (
            <option key={opt.value} value={opt.value}>
              {opt.label}
            </option>
          ))}
        </select>

        <div className="flex-1" />

        {/* Column visibility toggles */}
        {(["price", "size", "total"] as DomColumn[]).map((col) => (
          <label
            key={col}
            className="flex items-center gap-1 text-[10px] text-gray-500 cursor-pointer select-none"
          >
            <input
              type="checkbox"
              className="w-3 h-3"
              checked={visibleColumns.has(col)}
              onChange={() => toggleColumn(col)}
            />
            {col}
          </label>
        ))}

        {/* Auto-refresh toggle */}
        <button
          className={`text-[10px] px-1.5 py-0.5 rounded ${
            autoRefresh
              ? "bg-blue-100 text-blue-700"
              : "bg-gray-200 text-gray-500"
          }`}
          onClick={() => setAutoRefresh((p) => !p)}
          aria-label="Toggle auto-refresh"
          title="Toggle auto-refresh"
        >
          {autoRefresh ? "LIVE" : "PAUSED"}
        </button>

        {/* Manual refresh */}
        <button
          className="text-[10px] px-1.5 py-0.5 rounded bg-gray-200 text-gray-600 hover:bg-gray-300"
          onClick={handleRefresh}
          aria-label="Refresh now"
          title="Refresh now"
        >
          Refresh
        </button>
      </div>

      {/* Order book */}
      <div className="flex-1 overflow-hidden">
        {displayData ? (
          <OrderBook
            data={displayData}
            maxLevels={maxLevels}
            precision={precision}
          />
        ) : (
          <div className="flex h-full items-center justify-center text-gray-400 text-xs">
            Select a market to view depth
          </div>
        )}
      </div>

      {/* Status bar */}
      <div className="flex items-center justify-between px-2 py-1 border-t border-gray-200 bg-gray-50 text-[10px] text-gray-400 flex-shrink-0">
        <span>{selectedMarketLabel}</span>
        <span>
          {displayData?.depth ?? 0} levels &middot;{" "}
          {displayData?.lastUpdate
            ? new Date(displayData.lastUpdate).toLocaleTimeString()
            : "---"}
        </span>
      </div>
    </div>
  );
}

// Re-export OrderBook types for consumer convenience
export type { OrderBookData, BookLevel } from "@/components/order-book";
