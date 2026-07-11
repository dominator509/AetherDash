// Proto mirror: Service RPC request/response types
// Covers: venue/v1/adapter.proto, router/v1/router.proto,
//         guardian/v1/guardian.proto, brain/v1/brain.proto
import type { Ulid, MarketKey } from "./types.js";
import type { Order, RiskVerdict } from "./orders.js";
import type { BrainRef } from "./market_data.js";

// ── VenueAdapter (venue/v1/adapter.proto) ──
export interface ListMarketsRequest {
  filter?: string;
}

export interface GetMarketRequest {
  key: MarketKey;
}

export interface StreamTicksRequest {
  keys: MarketKey[];
}

export interface StreamBookRequest {
  key: MarketKey;
  depth?: number;
}

export interface CancelOrderRequest {
  venue_ref: string;
}

export interface CancelOrderResponse {
  cancelled: boolean;
}

export interface OrderAck {
  venue_ref: string;
  status: string;
}

export interface Balance {
  asset: string;
  free: string;
  locked: string;
}

export interface Balances {
  balances: Balance[];
}

export interface VenueHealth {
  status: string;
  lag_ms: number;
  rate_remaining: number;
}

// ── OrderRouter (router/v1/router.proto) ──
export interface RouterResult {
  order?: Order;
  verdict?: RiskVerdict;
}

export interface CancelRequest {
  order_id: Ulid;
}

export interface CancelResponse {
  cancelled: boolean;
}

export interface StatusRequest {
  order_id: Ulid;
}

// ── WalletGuardian (guardian/v1/guardian.proto) ──
export type ProposalStatus = "unspecified" | "pending" | "auto_approved" | "denied";

export interface TxSpec {
  to: string;
  value: string;
  data: string;
  chain_id: string;
}

export interface Approval {
  totp: string;
  ts: string;
}

export interface ApproveProposalRequest {
  id: string;
  approval: Approval;
}

export interface ProposalRequest {
  id: string;
}

export interface Proposal {
  id: string;
  status: ProposalStatus;
  policy_trace: string;
}

// ── Brain (brain/v1/brain.proto) ──
export interface ObjectDraft {
  kind: string;
  content: string;
  source: string;
}

export interface RecallRequest {
  query: string;
  k?: number;
  filters?: string;
}

export interface ScoredRef {
  ref: BrainRef;
  score: number;
}

export interface RecallResponse {
  refs: ScoredRef[];
}

export interface ExplainRequest {
  opportunity_id: Ulid;
}

export interface ExplainTree {
  tree_json: string;
}
