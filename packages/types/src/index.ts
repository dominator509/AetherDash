// AETHER Terminal — TypeScript domain type mirrors
// mirrors: crates/aether-core (SPEC-001, all 17 types)
// D7: hand-mirrored types reference proto message names

export type Ulid = string;                    // mirrors: aether.core.v1.Ulid
export type MarketKey = string;               // mirrors: aether.core.v1.MarketKey, "mkt:{venue}:{native_id}"
export type VenueId = string;                 // mirrors: aether.core.v1.VenueId

export interface Money {
  amount: string; currency: string;           // mirrors: aether.core.v1.Money
}

export type InstrumentKind =
  | "binary_contract" | "categorical_contract" | "scalar_contract"
  | "equity" | "option" | "perp" | "spot";

export type MarketStatus = "open" | "halted" | "closed" | "resolved";

export type PriceSemantics =
  | { kind: "probability"; tick_size: string }
  | { kind: "scalar"; unit: string; min: string; max: string }
  | { kind: "currency" };

export interface Market {
  key: MarketKey; venue: VenueId; kind: InstrumentKind;
  title: string; description_ref: string; status: MarketStatus;
  close_ts?: string; resolve_ts?: string; outcome?: string;
  jurisdiction_flags: string[]; venue_ref: unknown; meta: unknown;
}

export type QuoteSource = "stream" | "poll" | "snapshot";

export interface Quote {
  market: MarketKey; bid?: string; ask?: string; mid?: string;
  last?: string; bid_size?: string; ask_size?: string;
  ts: string; source: QuoteSource; seq?: number;
}

export interface BookLevel { price: string; size: string; }

export interface OrderBook {
  market: MarketKey; bids: BookLevel[]; asks: BookLevel[];
  depth: number; ts: string; seq?: number;
}

export type Side = "buy" | "sell" | "buy_no" | "sell_no";
export type OrderType = "limit" | "market";
export type TimeInForce = "ioc" | "gtc" | "day";

export interface OrderIntent {
  id: Ulid; market: MarketKey; side: Side; order_type: OrderType;
  limit_price?: string; size: string; size_unit: string; tif: TimeInForce;
  paper: boolean; origin: { kind: string; tier: number; actor_id: Ulid };
  quote_snapshot: Quote; caps_version: Ulid; created_ts: string;
}

export interface RiskVerdict {
  intent_id: Ulid; verdict: "allow" | "deny";
  reasons: { code: string; detail: string }[]; ts: string;
}

export interface Order {
  order_id: Ulid; market: MarketKey; side: Side; price: string;
  size: string; fee: Money; venue_ref: unknown; ts: string; paper: boolean;
}

export interface Fill {
  order_id: Ulid; market: MarketKey; side: Side; price: string;
  size: string; fee: Money; venue_ref: unknown; ts: string; paper: boolean;
}

export interface Position {
  market: MarketKey; side_exposure: string; avg_price: string;
  size: string; realized_pnl: Money; unrealized_pnl: Money; ts: string;
}

export interface CapsSnapshot {
  version: Ulid; per_order_max: Money; daily_max: Money;
  per_venue: Record<string, unknown>; per_kind: Record<string, unknown>;
}

export type OpportunityKind = "arbitrage" | "value" | "catalyst" | "hedge";

export interface EdgeDecomposition {
  gross_spread: string; fees: string; slippage_est: string;
  funding_cost: string; gas_cost: string; bridge_cost: string;
  settlement_mismatch_discount: string; liquidity_haircut: string;
  staleness_penalty: string; confidence_penalty: string; net_edge: string;
}

export interface Opportunity {
  id: Ulid; kind: OpportunityKind; legs: { market: MarketKey; side: Side; target_price?: string; size_hint?: string }[];
  gross_edge: string; edge: EdgeDecomposition; confidence: string;
  detected_ts: string; expires_ts?: string;
  explain_ref: { object_id: Ulid; provenance_hash: string }; trace_id: Ulid;
}

export interface AuditEvent {
  seq: number; prev_hash: string; hash: string; ts: string;
  actor: string; action: string; subject: string; payload_hash: string;
}

export interface ErrorEnvelope {
  code: string; message: string; retryable: boolean;
  trace_id: Ulid; details?: string;
}

// Canonical JSON: deterministic field order, decimals as strings, omit nulls
export function canonicalJson(value: unknown): string {
  return JSON.stringify(value, (_key, val) => (val === null ? undefined : val));
}
