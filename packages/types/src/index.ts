// AETHER Terminal — TypeScript domain type mirrors
// mirrors: crates/aether-core — D7: hand-mirrored types reference proto message names

export type Ulid = string;  // mirrors: aether.core.v1.Ulid

export type MarketKey = string;  // mirrors: aether.core.v1.MarketKey, "mkt:{venue}:{native_id}"

export type VenueId = string;  // mirrors: aether.core.v1.VenueId

export interface Money {
  amount: string;    // decimal string
  currency: string;  // ISO-4217 or USDC|USDT|ETH|...
}

export type InstrumentKind =
  | "binary_contract"
  | "categorical_contract"
  | "scalar_contract"
  | "equity"
  | "option"
  | "perp"
  | "spot";

export type MarketStatus = "open" | "halted" | "closed" | "resolved";

export type PriceSemantics =
  | { kind: "probability"; tick_size: string }
  | { kind: "scalar"; unit: string; min: string; max: string }
  | { kind: "currency" };

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

export interface Confidence {
  value: string;  // decimal string, 0..=1
}

// Canonical JSON: deterministic field order, decimals as strings, omit nulls
export function canonicalJson(value: unknown): string {
  return JSON.stringify(value, (_key, val) => (val === null ? undefined : val));
}
