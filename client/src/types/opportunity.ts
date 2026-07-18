// SPEC-012 Opportunity and EdgeDecomposition types for the client.
// Mirrors aether-core types — wire format is canonical JSON.

/** A market key: mkt:{venue}:{native_id} */
export type MarketKey = string;

/** The 11-component edge decomposition per SPEC-012. */
export interface EdgeDecomposition {
  gross_spread: string; // Decimal string
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

/** Opportunity lifecycle states. */
export type LifecycleState =
  "detected" | "scored" | "surfaced" | "accepted" | "executed" | "closed" | "ignored" | "expired";

/** A detected arbitrage opportunity. */
export interface Opportunity {
  id: string; // ULID
  kind: "arbitrage" | "value" | "catalyst" | "hedge";
  legs: OpportunityLeg[];
  gross_edge: string;
  edge: EdgeDecomposition;
  confidence: string; // Decimal 0..1
  detected_ts: string; // RFC3339
  expires_ts: string | null;
  state: LifecycleState;
  explain_ref: BrainRef | null;
  trace_id: string | null;
}

/** A single leg of an opportunity. */
export interface OpportunityLeg {
  market: MarketKey;
  side: "buy" | "sell";
  target_price: string | null;
  size_hint: string | null;
}

/** Brain object reference for explain views. */
export interface BrainRef {
  object_id: string;
  provenance_hash: string;
}

/** Display hints attached to feed items. */
export interface FeedDisplayHints {
  venue_a: string;
  venue_b: string;
  asset_label: string;
  quote_age_ms: number;
  tick_stale_ms: number;
  volume_24h: number | null;
}

/** A feed item — Opportunity + display hints. */
export interface FeedItem {
  opportunity: Opportunity;
  hints: FeedDisplayHints;
}
