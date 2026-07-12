// Proto mirror: aether/core/v1/orders.proto — Order types
import type { Ulid, MarketKey, Money } from "./types.js";
import type { Quote } from "./market_data.js";

// ── Enums ──
export type Side = "buy" | "sell" | "buy_no" | "sell_no";
export type OrderType = "limit" | "market";
export type TimeInForce = "ioc" | "gtc" | "day";
export type SizeUnit = "contracts" | "shares" | "base" | "quote";
export type OriginKind = "human" | "agent" | "automation";
export type RiskVerdictStatus = "allow" | "deny";
export type RiskReasonCode =
  | "liveness"
  | "price_drift"
  | "balance"
  | "venue_health"
  | "cap_exceeded"
  | "jurisdiction"
  | "live_disabled";

// ── Messages ──
export interface Origin {
  kind: OriginKind;
  tier: number;
  actor_id: Ulid;
}

export interface RiskReason {
  code: RiskReasonCode;
  detail: string;
}

export interface OrderIntent {
  id: Ulid;
  market: MarketKey;
  side: Side;
  order_type: OrderType;
  limit_price?: string;
  size: string;
  size_unit: SizeUnit;
  tif: TimeInForce;
  paper: boolean;
  origin: Origin;
  quote_snapshot: Quote;
  caps_version: Ulid;
  created_ts: string;
}

export interface RiskVerdict {
  intent_id: Ulid;
  verdict: RiskVerdictStatus;
  reasons: RiskReason[];
  ts: string;
}

export interface Order {
  order_id: Ulid;
  market: MarketKey;
  side: Side;
  price: string;
  size: string;
  fee: Money;
  venue_ref: string;
  ts: string;
  paper: boolean;
}

export interface Fill {
  order_id: Ulid;
  market: MarketKey;
  side: Side;
  price: string;
  size: string;
  fee: Money;
  venue_ref: string;
  ts: string;
  paper: boolean;
}

export interface Position {
  market: MarketKey;
  side_exposure: string;
  avg_price: string;
  size: string;
  realized_pnl: Money;
  unrealized_pnl: Money;
  ts: string;
}

export interface CapsSnapshot {
  version: Ulid;
  per_order_max: Money;
  daily_max: Money;
  per_venue: Record<string, string>;
  per_kind: Record<string, string>;
}
