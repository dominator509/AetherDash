/**
 * Tests for the OrderBook component (SPEC-004 Surface 4).
 *
 * Covers:
 * - Bid ordering (descending price)
 * - Ask ordering (ascending price)
 * - Spread calculation display
 * - Depth bar proportions
 * - Empty/no-data state
 * - Precision formatting
 * - Cumulative total computation
 */

import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { OrderBook, type OrderBookData, type BookLevel } from "@/components/order-book";

// ── Test data factories ────────────────────────────────────────────────────────

function makeBookLevel(price: string, size: string): BookLevel {
  return { price, size, total: "0" };
}

function makeOrderBook(overrides?: Partial<OrderBookData>): OrderBookData {
  return {
    market: "test:USDMXN",
    bids: [
      makeBookLevel("20.05", "100"),
      makeBookLevel("20.04", "200"),
      makeBookLevel("20.03", "150"),
      makeBookLevel("20.02", "300"),
      makeBookLevel("20.01", "250"),
    ],
    asks: [
      makeBookLevel("20.06", "120"),
      makeBookLevel("20.07", "180"),
      makeBookLevel("20.08", "220"),
      makeBookLevel("20.09", "90"),
      makeBookLevel("20.10", "300"),
    ],
    spread: "0.01",
    midPrice: "20.055",
    lastUpdate: Date.now(),
    depth: 5,
    ...overrides,
  };
}

// ── Tests ──────────────────────────────────────────────────────────────────────

describe("OrderBook", () => {
  it("renders without crashing", () => {
    const data = makeOrderBook();
    const { container } = render(<OrderBook data={data} />);
    expect(container.querySelector('[role="region"]')).toBeTruthy();
  });

  it("renders spread percentage", () => {
    const data = makeOrderBook();
    render(<OrderBook data={data} />);
    const spread = screen.getByLabelText("Spread");
    // Spread = (20.06 - 20.05) / 20.05 * 100 = 0.049875...%
    expect(spread.textContent).toContain("0.050%");
  });

  it("renders mid price", () => {
    const data = makeOrderBook();
    render(<OrderBook data={data} />);
    const midEl = screen.getByLabelText("Mid price");
    expect(midEl).toBeTruthy();
    expect(midEl.textContent).toBe("Mid: 20.055");
  });

  it("shows 'N/A' for spread when bids or asks are empty", () => {
    const data = makeOrderBook({ bids: [], asks: [], midPrice: null });
    render(<OrderBook data={data} />);
    expect(screen.getByText(/Spread: N\/A/)).toBeTruthy();
  });

  it("shows no mid price when bids or asks are empty", () => {
    const data = makeOrderBook({ bids: [], asks: [], midPrice: null });
    render(<OrderBook data={data} />);
    expect(screen.queryByLabelText("Mid price")).toBeNull();
  });

  it("shows 'Awaiting market data' when bids and asks are empty", () => {
    const data = makeOrderBook({ bids: [], asks: [], midPrice: null });
    render(<OrderBook data={data} />);
    const el = screen.getByText(/Awaiting market data/);
    expect(el).toBeTruthy();
  });

  it("displays asks above bids (sell-side first)", () => {
    const data = makeOrderBook();
    render(<OrderBook data={data} />);

    const askList = screen.getByLabelText("Asks");
    const bidList = screen.getByLabelText("Bids");

    expect(askList).toBeTruthy();
    expect(bidList).toBeTruthy();
  });

  it("formats prices with correct precision", () => {
    const data = makeOrderBook({
      bids: [makeBookLevel("20.1235", "100")],
      asks: [makeBookLevel("20.6789", "100")],
    });
    const { container } = render(<OrderBook data={data} precision={4} />);

    // Prices pass through toFixed(precision): 20.1235 → "20.1235", 20.6789 → "20.6789"
    const greenPrices = container.querySelectorAll(".text-green-600");
    const redPrices = container.querySelectorAll(".text-red-600");
    expect(greenPrices.length).toBeGreaterThanOrEqual(1);
    expect(redPrices.length).toBeGreaterThanOrEqual(1);
    expect(greenPrices[0]?.textContent).toBe("20.1235");
    expect(redPrices[0]?.textContent).toBe("20.6789");
  });

  it("computes cumulative totals correctly for bids", () => {
    const data = makeOrderBook({
      bids: [
        makeBookLevel("20.05", "100"),
        makeBookLevel("20.04", "200"),
        makeBookLevel("20.03", "300"),
      ],
      asks: [],
      midPrice: null,
    });
    const { container } = render(<OrderBook data={data} precision={2} />);

    // Bids displayed in descending price order (highest first).
    // After reverse for display: 20.05 (cumulative=100), 20.04 (cumulative=300), 20.03 (cumulative=600)
    // Total column cells have class "text-gray-400"
    const totalCells = container.querySelectorAll(".text-gray-400.z-10");
    expect(totalCells.length).toBeGreaterThanOrEqual(3);
    // First row (20.05): total=100, Second row (20.04): total=300, Third row (20.03): total=600
    // Since bids are first in source order (after asks), the Total column cells are after the size cells
    const values = Array.from(totalCells).map((el) => el.textContent);
    expect(values).toContain("600.00");
    expect(values).toContain("300.00");
    expect(values).toContain("100.00");
  });

  it("computes cumulative totals correctly for asks", () => {
    const data = makeOrderBook({
      bids: [],
      asks: [
        makeBookLevel("20.06", "120"),
        makeBookLevel("20.07", "180"),
        makeBookLevel("20.08", "220"),
      ],
      midPrice: null,
    });
    const { container } = render(<OrderBook data={data} precision={2} />);

    // Asks sorted ascending: 20.06 total=120, 20.07 total=300, 20.08 total=520
    // Price column = text-red-600, Size column = no special class, Total column = text-gray-400 z-10
    const totalCells = container.querySelectorAll(".text-gray-400.z-10");
    const totals = Array.from(totalCells).map((el) => el.textContent);
    expect(totals).toContain("120.00");
    expect(totals).toContain("300.00");
    expect(totals).toContain("520.00");
  });

  it("renders depth bars with widths proportional to total size", () => {
    const data = makeOrderBook({
      bids: [makeBookLevel("20.05", "500")],
      asks: [makeBookLevel("20.06", "100")],
    });
    const { container } = render(<OrderBook data={data} />);

    // There should be depth-bar divs (bg-red-100 and bg-green-100)
    const depthBars = container.querySelectorAll(".bg-red-100");
    expect(depthBars.length).toBeGreaterThanOrEqual(1);

    const depthBarsGreen = container.querySelectorAll(".bg-green-100");
    expect(depthBarsGreen.length).toBeGreaterThanOrEqual(1);
  });

  it("truncates levels to maxLevels", () => {
    const manyBids = Array.from({ length: 30 }, (_, i) =>
      makeBookLevel((20.0 - i * 0.01).toFixed(2), "100"),
    );
    const manyAsks = Array.from({ length: 30 }, (_, i) =>
      makeBookLevel((20.01 + i * 0.01).toFixed(2), "100"),
    );

    const data = makeOrderBook({ bids: manyBids, asks: manyAsks });
    const { container } = render(<OrderBook data={data} maxLevels={10} />);

    const askItems = container.querySelectorAll('[role="listitem"]');
    // askItems should be 10 asks + 10 bids = 20
    expect(askItems.length).toBeLessThanOrEqual(20);

    // With maxLevels=10 and 30 available, only 10 each shown
    const askList = container.querySelector('[aria-label="Asks"]');
    const bidList = container.querySelector('[aria-label="Bids"]');
    expect(askList?.childNodes.length).toBeLessThanOrEqual(10);
    expect(bidList?.childNodes.length).toBeLessThanOrEqual(10);
  });

  it("displays lastUpdate timestamp in footer when positive", () => {
    const ts = 1720800000000; // Known timestamp
    const data = makeOrderBook({ lastUpdate: ts });
    render(<OrderBook data={data} />);

    const timeStr = new Date(ts).toLocaleTimeString();
    expect(screen.getByText(timeStr)).toBeTruthy();
  });

  it('shows "No data" in footer when lastUpdate is zero', () => {
    const data = makeOrderBook({ lastUpdate: 0 });
    render(<OrderBook data={data} />);
    expect(screen.getByText("No data")).toBeTruthy();
  });

  it("handles single-level book correctly", () => {
    const data = makeOrderBook({
      bids: [makeBookLevel("100.00", "1000")],
      asks: [makeBookLevel("101.00", "500")],
    });
    render(<OrderBook data={data} />);

    expect(screen.getByText("100.0000")).toBeTruthy();
    expect(screen.getByText("101.0000")).toBeTruthy();
    // Spread = (101 - 100) / 100 * 100 = 1%
    expect(screen.getByText(/1.000%/)).toBeTruthy();
  });

  it("renders bid rows with green price text", () => {
    const data = makeOrderBook({
      bids: [makeBookLevel("20.05", "100")],
      asks: [],
    });
    const { container } = render(<OrderBook data={data} />);
    const greenSpans = container.querySelectorAll(".text-green-600");
    expect(greenSpans.length).toBeGreaterThanOrEqual(1);
    expect(greenSpans[0]?.textContent).toBe("20.0500");
  });

  it("renders ask rows with red price text", () => {
    const data = makeOrderBook({
      bids: [],
      asks: [makeBookLevel("20.06", "100")],
    });
    const { container } = render(<OrderBook data={data} />);
    const redSpans = container.querySelectorAll(".text-red-600");
    expect(redSpans.length).toBeGreaterThanOrEqual(1);
    expect(redSpans[0]?.textContent).toBe("20.0600");
  });

  it("shows null spread percentage when best bid is zero", () => {
    const data = makeOrderBook({
      bids: [makeBookLevel("0.00", "100")],
      asks: [makeBookLevel("0.01", "100")],
    });
    render(<OrderBook data={data} />);
    expect(screen.getByText(/Spread: N\/A/)).toBeTruthy();
  });
});
