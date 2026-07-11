// Proto mirror: aether/core/v1/market_data.proto — Market, quote, order book
// Also includes: aether/core/v1/opportunity.proto
import type { Ulid, MarketKey, VenueId, Confidence } from "./types.js";
import type { Side } from "./orders.js";

// ── Enums ──
export type InstrumentKind =
  | "binary_contract"
  | "categorical_contract"
  | "scalar_contract"
  | "equity"
  | "option"
  | "perp"
  | "spot";

export type MarketStatus = "open" | "halted" | "closed" | "resolved";

export type QuoteSource = "stream" | "poll" | "snapshot";

export type OpportunityKind = "arbitrage" | "value" | "catalyst" | "hedge";

// ── Messages — market_data ──
export interface PriceSemantics {
  kind: string;
  tick_size?: string;
  unit?: string;
  min?: string;
  max?: string;
}

export interface Market {
  key: MarketKey;
  venue: VenueId;
  kind: InstrumentKind;
  title: string;
  description_ref: string;
  status: MarketStatus;
  close_ts?: string;
  resolve_ts?: string;
  outcome?: string;
  jurisdiction_flags: string[];
  venue_ref: string;
  meta: string;
}

export interface Quote {
  market: MarketKey;
  bid?: string;
  ask?: string;
  mid?: string;
  last?: string;
  bid_size?: string;
  ask_size?: string;
  ts: string;
  source: QuoteSource;
  seq?: number;
}

export interface BookLevel {
  price: string;
  size: string;
}

export interface OrderBook {
  market: MarketKey;
  bids: BookLevel[];
  asks: BookLevel[];
  depth: number;
  ts: string;
  seq?: number;
}

// ── Messages — opportunity ──
export interface OpportunityLeg {
  market: MarketKey;
  side: Side;
  target_price?: string;
  size_hint?: string;
}

export interface BrainRef {
  object_id: Ulid;
  provenance_hash: string;
}

export interface EdgeDecomposition {
  gross_spread: string;
  fees: string;
  slippage_est: string;
  funding_cost: string;
  gas_cost: string;
  bridge_cost: string;
  settlement_mismatch_discount: string;
  liquidity_haircut: string;
  staleness_penalty: string;
  confidence_penalty: string;
  net_edge: string;
}

export interface Opportunity {
  id: Ulid;
  kind: OpportunityKind;
  legs: OpportunityLeg[];
  gross_edge: string;
  edge: EdgeDecomposition;
  confidence: Confidence;
  detected_ts: string;
  expires_ts?: string;
  explain_ref: BrainRef;
  trace_id: Ulid;
}
