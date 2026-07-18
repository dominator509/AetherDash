/**
 * Tests for EP-102 Opportunity Feed data structures, feed state management,
 * and component rendering invariants.
 *
 * Covers:
 * - EdgeDecomposition: all 11 components render including explicit zeros (SPEC-012)
 * - Feed state: lifecycle coalescing (no duplicate cards on update)
 * - Feed state: expiry drops stale terminal-state items
 * - Feed state: getOrderedItems returns items in insertion order (newest first)
 * - StalenessChip: three variants (fresh, aging, stale) based on ratio
 * - EdgeTable: renders all rows, NET EDGE is bolded
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { StalenessChip } from "../components/staleness-chip";
import { EdgeTable } from "../components/edge-table";
import {
  createFeedState,
  upsertFeedItem,
  expireFeedItems,
  setFeedDegraded,
  getOrderedItems,
} from "../state/feed";
import type {
  FeedItem,
  Opportunity,
  EdgeDecomposition,
  FeedDisplayHints,
} from "../types/opportunity";

// ── Helpers ──────────────────────────────────────────────────────────────────

function makeEdge(overrides?: Partial<EdgeDecomposition>): EdgeDecomposition {
  return {
    gross_spread: "0.005",
    fees: "0.001",
    slippage_est: "0.0005",
    funding_cost: "0",
    gas_cost: "0.002",
    bridge_cost: "0",
    settlement_mismatch_discount: "0",
    liquidity_haircut: "0.001",
    staleness_penalty: "0",
    confidence_penalty: "0.0003",
    net_edge: "0.0002",
    ...overrides,
  };
}

function makeOpportunity(id: string, overrides?: Partial<Opportunity>): Opportunity {
  return {
    id,
    kind: "arbitrage",
    legs: [
      { market: "mkt:venue_a:asset", side: "buy", target_price: "100.00", size_hint: "1.0" },
      { market: "mkt:venue_b:asset", side: "sell", target_price: "100.50", size_hint: "1.0" },
    ],
    gross_edge: "0.005",
    edge: makeEdge(),
    confidence: "0.85",
    detected_ts: new Date().toISOString(),
    expires_ts: null,
    state: "surfaced",
    explain_ref: null,
    trace_id: "trace-abc-123",
    ...overrides,
  };
}

function makeHints(overrides?: Partial<FeedDisplayHints>): FeedDisplayHints {
  return {
    venue_a: "VenueA",
    venue_b: "VenueB",
    asset_label: "BTC/USD",
    quote_age_ms: 150,
    tick_stale_ms: 500,
    volume_24h: 10000,
    ...overrides,
  };
}

function makeFeedItem(
  id: string,
  overrides?: { opp?: Partial<Opportunity>; hints?: Partial<FeedDisplayHints> },
): FeedItem {
  return {
    opportunity: makeOpportunity(id, overrides?.opp),
    hints: makeHints(overrides?.hints),
  };
}

// ── EdgeDecomposition: all 11 components ─────────────────────────────────────

describe("EdgeDecomposition structure", () => {
  it("has exactly 11 required fields", () => {
    const edge = makeEdge();
    const fields = Object.keys(edge) as (keyof EdgeDecomposition)[];
    expect(fields).toHaveLength(11);
    expect(fields).toEqual([
      "gross_spread",
      "fees",
      "slippage_est",
      "funding_cost",
      "gas_cost",
      "bridge_cost",
      "settlement_mismatch_discount",
      "liquidity_haircut",
      "staleness_penalty",
      "confidence_penalty",
      "net_edge",
    ]);
  });

  it("all fields are decimal strings", () => {
    const edge = makeEdge();
    for (const value of Object.values(edge)) {
      expect(value).toEqual(expect.any(String));
      expect(isNaN(Number(value))).toBe(false);
    }
  });
});

// ── EdgeTable renders all 11 components ─────────────────────────────────-----

describe("EdgeTable", () => {
  it("renders all 11 component rows", () => {
    const edge = makeEdge();
    render(<EdgeTable edge={edge} />);

    expect(screen.getByText("Gross Spread")).toBeTruthy();
    expect(screen.getByText("Fees")).toBeTruthy();
    expect(screen.getByText("Slippage (est)")).toBeTruthy();
    expect(screen.getByText("Funding Cost")).toBeTruthy();
    expect(screen.getByText("Gas Cost")).toBeTruthy();
    expect(screen.getByText("Bridge Cost")).toBeTruthy();
    expect(screen.getByText("Settlement Mismatch")).toBeTruthy();
    expect(screen.getByText("Liquidity Haircut")).toBeTruthy();
    expect(screen.getByText("Staleness Penalty")).toBeTruthy();
    expect(screen.getByText("Confidence Penalty")).toBeTruthy();
    expect(screen.getByText("NET EDGE")).toBeTruthy();
  });

  it('shows explicit zero values with "(not applicable)" text', () => {
    const edge = makeEdge({
      funding_cost: "0",
      bridge_cost: "0",
      settlement_mismatch_discount: "0",
      staleness_penalty: "0",
    });
    render(<EdgeTable edge={edge} />);

    const zeroRows = screen.getAllByText(/not applicable/);
    expect(zeroRows.length).toBeGreaterThanOrEqual(4);
  });

  it('does NOT show "(not applicable)" for non-zero values', () => {
    const edge = makeEdge();
    render(<EdgeTable edge={edge} />);

    const nonZeroValues = new Set(
      Object.values(edge).filter((component) => Number(component) !== 0),
    );
    for (const value of nonZeroValues) {
      for (const cell of screen.getAllByText(value)) {
        expect(cell.textContent).toBe(value);
        expect(cell.textContent).not.toContain("not applicable");
      }
    }
  });

  it("NET EDGE row is visually distinct (bold style)", () => {
    const edge = makeEdge();
    render(<EdgeTable edge={edge} />);

    const netEdgeRow = screen.getByText("NET EDGE");
    expect(netEdgeRow.tagName).toBe("TD");
    const parentRow = netEdgeRow.closest("tr");
    expect(parentRow?.className).toContain("font-bold");
  });
});

// ── Feed state: lifecycle coalescing ─────────────────────────────────────────

describe("Feed state management", () => {
  let state: ReturnType<typeof createFeedState>;

  beforeEach(() => {
    state = createFeedState();
  });

  it("starts empty", () => {
    expect(state.items.size).toBe(0);
    expect(state.order).toHaveLength(0);
    expect(state.lastUpdate).toBe(0);
    expect(state.degraded).toBe(false);
  });

  it("inserts a new feed item at the front", () => {
    const item = makeFeedItem("opp-001");
    upsertFeedItem(state, item);

    expect(state.items.size).toBe(1);
    expect(state.order).toEqual(["opp-001"]);
    expect(state.lastUpdate).toBeGreaterThan(0);
  });

  it("coalesces lifecycle update — no duplicate entry", () => {
    const item1 = makeFeedItem("opp-001", { opp: { state: "surfaced" } });
    upsertFeedItem(state, item1);

    const item2 = makeFeedItem("opp-001", { opp: { state: "accepted" } });
    upsertFeedItem(state, item2);

    // Only one entry in the map, order unchanged
    expect(state.items.size).toBe(1);
    expect(state.order).toEqual(["opp-001"]);
    expect(state.items.get("opp-001")!.opportunity.state).toBe("accepted");
  });

  it("maintains separate entries for distinct opportunity IDs", () => {
    upsertFeedItem(state, makeFeedItem("opp-001"));
    upsertFeedItem(state, makeFeedItem("opp-002"));

    expect(state.items.size).toBe(2);
    expect(state.order).toEqual(["opp-002", "opp-001"]); // newest first
  });

  it("returns items in insertion order via getOrderedItems", () => {
    upsertFeedItem(state, makeFeedItem("opp-001"));
    upsertFeedItem(state, makeFeedItem("opp-002"));
    upsertFeedItem(state, makeFeedItem("opp-003"));

    const ordered = getOrderedItems(state);
    expect(ordered).toHaveLength(3);
    expect(ordered[0]!.opportunity.id).toBe("opp-003");
    expect(ordered[1]!.opportunity.id).toBe("opp-002");
    expect(ordered[2]!.opportunity.id).toBe("opp-001");
  });

  it("removes expired items from the feed after max age", () => {
    vi.useFakeTimers();
    const past = new Date(Date.now() - 100_000).toISOString();
    const item = makeFeedItem("opp-expired", {
      opp: { state: "expired", detected_ts: past },
    });
    upsertFeedItem(state, item);

    // maxAgeMs = 50_000; detected 100_000ms ago → expired
    expireFeedItems(state, 50_000);
    expect(state.items.has("opp-expired")).toBe(false);
    expect(state.order).not.toContain("opp-expired");
    vi.useRealTimers();
  });

  it("keeps expired items if within max age", () => {
    vi.useFakeTimers();
    const recent = new Date(Date.now() - 10_000).toISOString();
    const item = makeFeedItem("opp-recent", {
      opp: { state: "expired", detected_ts: recent },
    });
    upsertFeedItem(state, item);

    // maxAgeMs = 50_000; detected 10_000ms ago → still within window
    expireFeedItems(state, 50_000);
    expect(state.items.has("opp-recent")).toBe(true);
    vi.useRealTimers();
  });

  it("does not remove non-terminal items regardless of age", () => {
    vi.useFakeTimers();
    const old = new Date(Date.now() - 200_000).toISOString();
    const item = makeFeedItem("opp-old", {
      opp: { state: "surfaced", detected_ts: old },
    });
    upsertFeedItem(state, item);

    expireFeedItems(state, 50_000);
    expect(state.items.has("opp-old")).toBe(true);
    vi.useRealTimers();
  });

  it("sets and clears degraded mode", () => {
    setFeedDegraded(state, true);
    expect(state.degraded).toBe(true);

    setFeedDegraded(state, false);
    expect(state.degraded).toBe(false);
  });
});

// ── StalenessChip variants ───────────────────────────────────────────────────

describe("StalenessChip", () => {
  it('shows "Fresh" when ratio <= 1', () => {
    render(<StalenessChip quoteAgeMs={100} tickStaleMs={500} />);
    expect(screen.getByText(/Fresh/)).toBeTruthy();
  });

  it('shows "Aging" when ratio > 1 and <= 2', () => {
    render(<StalenessChip quoteAgeMs={600} tickStaleMs={500} />);
    expect(screen.getByText(/Aging/)).toBeTruthy();
  });

  it('shows "Stale" when ratio > 2', () => {
    render(<StalenessChip quoteAgeMs={1500} tickStaleMs={500} />);
    expect(screen.getByText(/Stale/)).toBeTruthy();
  });

  it("includes quote age in milliseconds label", () => {
    render(<StalenessChip quoteAgeMs={750} tickStaleMs={500} />);
    expect(screen.getByText(/750ms/)).toBeTruthy();
  });

  it("has accessible aria-label describing quote health", () => {
    render(<StalenessChip quoteAgeMs={100} tickStaleMs={500} />);
    const chip = screen.getByText(/Fresh/).closest("span");
    expect(chip?.getAttribute("aria-label")).toMatch(/Quote:/);
  });

  it("renders at sm size without error", () => {
    render(<StalenessChip quoteAgeMs={200} tickStaleMs={1000} size="sm" />);
    expect(screen.getByText(/Fresh/)).toBeTruthy();
  });
});

// ── FeedSurface empty / degraded states ──────────────────────────────────────
// These are tested at the component level in FeedSurface — the
// empty-state and degraded-state branches are self-contained render paths.

// ── ExplainSurface structure ─────────────────────────────────────────────────
// Layout: Summary -> EdgeDecomposition table -> Evidence section -> Legs -> Raw JSON.
// These are tested in explain-surface.test.ts (EP-201).
