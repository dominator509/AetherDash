// AETHER Terminal — TypeScript domain type mirrors
// mirrors: crates/aether-core (SPEC-001, all 17 types)
// D7: hand-mirrored types reference proto message names

// ── Brand type helper ────────────────────────────────────────────────────
type Brand<T, B extends string> = T & { __brand: B };

// ── Branded scalar types with runtime validation ─────────────────────────
// mirrors: crates/aether-core/src/ids.rs

export type Ulid = Brand<string, "Ulid">;
export function Ulid(s: string): Ulid {
  if (!/^[0-9A-HJKMNP-TV-Z]{26}$/.test(s)) throw new Error(`Invalid Ulid: "${s}"`);
  return s as Ulid;
}

export type MarketKey = Brand<string, "MarketKey">;
export function MarketKey(s: string): MarketKey {
  if (!/^mkt:[a-z0-9]+:.+$/.test(s)) throw new Error(`Invalid MarketKey: "${s}"`);
  return s as MarketKey;
}

export type VenueId = Brand<string, "VenueId">;
export function VenueId(s: string): VenueId {
  if (!/^[a-z0-9]+$/.test(s) || s.length === 0) throw new Error(`Invalid VenueId: "${s}"`);
  return s as VenueId;
}

// ── Closed unions for enum-like string fields ────────────────────────────
// mirrors: crates/aether-core (enums with #[serde(rename_all = "snake_case")])

export type SizeUnit = "contracts" | "shares" | "base" | "quote";
export type OriginKind = "user" | "alert_action" | "agent" | "automation";
export type RiskVerdictStatus = "allow" | "deny";
export type RiskReasonCode =
  | "liveness"
  | "price_drift"
  | "balance"
  | "venue_health"
  | "cap_exceeded"
  | "jurisdiction"
  | "live_disabled";
export type ErrorCode =
  | "invalid_argument"
  | "unauthenticated"
  | "permission_denied"
  | "not_found"
  | "failed_precondition"
  | "unavailable"
  | "deadline_exceeded"
  | "quarantined"
  | "internal";

// ── Interface types ──────────────────────────────────────────────────────

export interface Money {
  amount: string;
  currency: string;
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
  venue_ref: unknown;
  meta: unknown;
}

export type QuoteSource = "stream" | "poll" | "snapshot";

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

export type Side = "buy" | "sell" | "buy_no" | "sell_no";
export type OrderType = "limit" | "market";
export type TimeInForce = "ioc" | "gtc" | "day";

export interface Origin {
  kind: OriginKind;
  tier: number;
  actor_id: Ulid;
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

export interface RiskReason {
  code: RiskReasonCode;
  detail: string;
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
  venue_ref: unknown;
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
  venue_ref: unknown;
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
  per_venue: Record<string, unknown>;
  per_kind: Record<string, unknown>;
}

export type OpportunityKind = "arbitrage" | "value" | "catalyst" | "hedge";

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

export interface BrainRef {
  object_id: Ulid;
  provenance_hash: string;
}

export interface Opportunity {
  id: Ulid;
  kind: OpportunityKind;
  legs: { market: MarketKey; side: Side; target_price?: string; size_hint?: string }[];
  gross_edge: string;
  edge: EdgeDecomposition;
  confidence: string;
  detected_ts: string;
  expires_ts?: string;
  explain_ref: BrainRef;
  trace_id: Ulid;
}

export interface AuditEvent {
  seq: number;
  prev_hash: string;
  hash: string;
  ts: string;
  actor: string;
  action: string;
  subject: string;
  payload_hash: string;
}

export interface ErrorEnvelope {
  code: ErrorCode;
  message: string;
  retryable: boolean;
  trace_id: Ulid;
  details?: string;
}

// ── Canonical JSON ───────────────────────────────────────────────────────
// SPEC-001: deterministic field order, decimals as strings, keep nulls
// (omit only undefined fields — the default JSON.stringify behavior)

export function canonicalJson(value: unknown): string {
  return JSON.stringify(value, (_key, val) => (val === undefined ? undefined : val));
}

/** Recursively sort object keys for deterministic output of map-like objects. */
export function canonicalJsonSorted(value: unknown): string {
  return JSON.stringify(value, (_key, val) => {
    if (val === undefined) return undefined;
    if (val && typeof val === "object" && !Array.isArray(val)) {
      const sorted: Record<string, unknown> = {};
      for (const k of Object.keys(val as Record<string, unknown>).sort()) {
        sorted[k] = (val as Record<string, unknown>)[k] as unknown;
      }
      return sorted;
    }
    return val;
  });
}

// ── Validation helpers ───────────────────────────────────────────────────

const ERR_RETRYABLE: Record<string, boolean> = {
  invalid_argument: false,
  unauthenticated: false,
  permission_denied: false,
  not_found: false,
  failed_precondition: false,
  unavailable: true,
  deadline_exceeded: true,
  quarantined: false,
  internal: false,
};

function assertString(v: unknown, path: string): asserts v is string {
  if (typeof v !== "string") throw new Error(`${path}: expected string, got ${typeof v}`);
}

function assertNumber(v: unknown, path: string): asserts v is number {
  if (typeof v !== "number" || !Number.isFinite(v))
    throw new Error(`${path}: expected finite number, got ${typeof v}`);
}

function assertBoolean(v: unknown, path: string): asserts v is boolean {
  if (typeof v !== "boolean") throw new Error(`${path}: expected boolean, got ${typeof v}`);
}

function assertObject(v: unknown, path: string): asserts v is Record<string, unknown> {
  if (typeof v !== "object" || v === null || Array.isArray(v))
    throw new Error(`${path}: expected object, got ${typeof v}`);
}

function assertArray(v: unknown, path: string): asserts v is unknown[] {
  if (!Array.isArray(v)) throw new Error(`${path}: expected array, got ${typeof v}`);
}

function assertStringArray(v: unknown, path: string): void {
  assertArray(v, path);
  for (let i = 0; i < v.length; i++) {
    if (typeof v[i] !== "string") throw new Error(`${path}[${i}]: expected string`);
  }
}

function assertOneOf(v: string, allowed: readonly string[], path: string): void {
  if (!allowed.includes(v))
    throw new Error(`${path}: expected one of [${allowed.join(", ")}], got "${v}"`);
}

function validateUlid(v: unknown, path: string): void {
  assertString(v, path);
  if (!/^[0-9A-HJKMNP-TV-Z]{26}$/.test(v))
    throw new Error(`${path}: invalid ULID format, got "${v}"`);
}

function validateMarketKey(v: unknown, path: string): void {
  assertString(v, path);
  if (!/^mkt:[a-z0-9]+:.+$/.test(v)) throw new Error(`${path}: invalid MarketKey, got "${v}"`);
}

function validateTimestamp(v: unknown, path: string): void {
  assertString(v, path);
  if (!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/.test(v))
    throw new Error(`${path}: invalid timestamp, got "${v}"`);
}

function validateDecimal(v: unknown, path: string): void {
  assertString(v, path);
  if (v === "") throw new Error(`${path}: decimal string must not be empty`);
  const n = Number(v);
  if (!Number.isFinite(n)) throw new Error(`${path}: invalid decimal string "${v}"`);
}

// ── validateAndCanonicalize — single entry point for all 17 SPEC-001 types ──

/**
 * Validate `value` against the schema for `typ`, then return its
 * canonical JSON string (deterministic field order matching Rust's serde_json).
 * Throws on any validation failure.
 */
export function validateAndCanonicalize(typ: string, value: unknown): string {
  switch (typ) {
    // ── Scalar types ────────────────────────────────────────────
    case "Ulid": {
      validateUlid(value, "Ulid");
      return canonicalJson(value);
    }
    case "MarketKey": {
      validateMarketKey(value, "MarketKey");
      return canonicalJson(value);
    }
    case "Confidence": {
      assertString(value, "Confidence");
      const n = Number(value);
      if (!Number.isFinite(n) || n < 0 || n > 1)
        throw new Error(`Confidence: must be in [0,1], got "${value}"`);
      return canonicalJson(value);
    }

    // ── Money ───────────────────────────────────────────────────
    case "Money": {
      const v = value as Record<string, unknown>;
      assertObject(value, "Money");
      validateDecimal(v.amount, "Money.amount");
      assertString(v.currency, "Money.currency");
      if (v.currency === "") throw new Error("Money.currency: must not be empty");
      return canonicalJson({ amount: v.amount, currency: v.currency });
    }

    // ── EdgeDecomposition ───────────────────────────────────────
    case "EdgeDecomposition": {
      const e = value as Record<string, unknown>;
      assertObject(value, "EdgeDecomposition");
      const FIELDS = [
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
      ] as const;
      const COST_FIELDS = [
        "fees",
        "slippage_est",
        "funding_cost",
        "gas_cost",
        "bridge_cost",
        "settlement_mismatch_discount",
        "liquidity_haircut",
        "staleness_penalty",
        "confidence_penalty",
      ] as const;
      for (const f of FIELDS) {
        if (!(f in e)) throw new Error(`EdgeDecomposition: missing field "${f}"`);
        validateDecimal(e[f], `EdgeDecomposition.${f}`);
      }
      // Sum-law: net_edge === gross_spread - sum(costs)
      const vg = Number(e.gross_spread);
      let costSum = 0;
      for (const f of COST_FIELDS) costSum += Number(e[f]);
      const vn = Number(e.net_edge);
      if (Math.abs(vn - (vg - costSum)) > 0.0001)
        throw new Error(
          `EdgeDecomposition: sum law violation: net=${vn} !== gross=${vg} - costs=${costSum} (${vg - costSum})`,
        );
      return canonicalJson(e);
    }

    // ── Quote ───────────────────────────────────────────────────
    case "Quote": {
      const q = value as Record<string, unknown>;
      assertObject(value, "Quote");
      validateMarketKey(q.market, "Quote.market");
      if (q.bid !== undefined) validateDecimal(q.bid, "Quote.bid");
      if (q.ask !== undefined) validateDecimal(q.ask, "Quote.ask");
      if (q.mid !== undefined) validateDecimal(q.mid, "Quote.mid");
      if (q.last !== undefined) validateDecimal(q.last, "Quote.last");
      if (q.bid_size !== undefined) validateDecimal(q.bid_size, "Quote.bid_size");
      if (q.ask_size !== undefined) validateDecimal(q.ask_size, "Quote.ask_size");
      validateTimestamp(q.ts, "Quote.ts");
      assertString(q.source, "Quote.source");
      assertOneOf(q.source, ["stream", "poll", "snapshot"] as const, "Quote.source");
      if (q.seq !== undefined) assertNumber(q.seq, "Quote.seq");
      return canonicalJson(q);
    }

    // ── OrderBook ───────────────────────────────────────────────
    case "OrderBook": {
      const ob = value as Record<string, unknown>;
      assertObject(value, "OrderBook");
      validateMarketKey(ob.market, "OrderBook.market");
      assertArray(ob.bids, "OrderBook.bids");
      assertArray(ob.asks, "OrderBook.asks");
      for (let i = 0; i < ob.bids.length; i++) {
        const bl = ob.bids[i] as Record<string, unknown>;
        assertObject(bl, `OrderBook.bids[${i}]`);
        validateDecimal(bl.price, `OrderBook.bids[${i}].price`);
        validateDecimal(bl.size, `OrderBook.bids[${i}].size`);
      }
      for (let i = 0; i < ob.asks.length; i++) {
        const bl = ob.asks[i] as Record<string, unknown>;
        assertObject(bl, `OrderBook.asks[${i}]`);
        validateDecimal(bl.price, `OrderBook.asks[${i}].price`);
        validateDecimal(bl.size, `OrderBook.asks[${i}].size`);
      }
      assertNumber(ob.depth, "OrderBook.depth");
      validateTimestamp(ob.ts, "OrderBook.ts");
      if (ob.seq !== undefined) assertNumber(ob.seq, "OrderBook.seq");
      return canonicalJson(ob);
    }

    // ── OrderIntent ─────────────────────────────────────────────
    case "OrderIntent": {
      const oi = value as Record<string, unknown>;
      assertObject(value, "OrderIntent");
      validateUlid(oi.id, "OrderIntent.id");
      validateMarketKey(oi.market, "OrderIntent.market");
      assertString(oi.side, "OrderIntent.side");
      assertOneOf(oi.side, ["buy", "sell", "buy_no", "sell_no"] as const, "OrderIntent.side");
      assertString(oi.order_type, "OrderIntent.order_type");
      assertOneOf(oi.order_type, ["limit", "market"] as const, "OrderIntent.order_type");
      if (oi.limit_price !== undefined) validateDecimal(oi.limit_price, "OrderIntent.limit_price");
      validateDecimal(oi.size, "OrderIntent.size");
      assertString(oi.size_unit, "OrderIntent.size_unit");
      assertOneOf(
        oi.size_unit,
        ["contracts", "shares", "base", "quote"] as const,
        "OrderIntent.size_unit",
      );
      assertString(oi.tif, "OrderIntent.tif");
      assertOneOf(oi.tif, ["ioc", "gtc", "day"] as const, "OrderIntent.tif");
      assertBoolean(oi.paper, "OrderIntent.paper");
      // origin
      assertObject(oi.origin, "OrderIntent.origin");
      const org = oi.origin as Record<string, unknown>;
      assertString(org.kind, "OrderIntent.origin.kind");
      assertOneOf(
        org.kind,
        ["user", "alert_action", "agent", "automation"] as const,
        "OrderIntent.origin.kind",
      );
      assertNumber(org.tier, "OrderIntent.origin.tier");
      if (!Number.isInteger(org.tier) || (org.tier as number) < 1 || (org.tier as number) > 5)
        throw new Error(`OrderIntent.origin.tier: must be 1..=5, got ${org.tier}`);
      validateUlid(org.actor_id, "OrderIntent.origin.actor_id");
      // quote_snapshot
      assertObject(oi.quote_snapshot, "OrderIntent.quote_snapshot");
      // Validate nested Quote inline (simplified)
      const snap = oi.quote_snapshot as Record<string, unknown>;
      validateMarketKey(snap.market, "OrderIntent.quote_snapshot.market");
      validateTimestamp(snap.ts, "OrderIntent.quote_snapshot.ts");
      assertString(snap.source, "OrderIntent.quote_snapshot.source");
      assertOneOf(
        snap.source,
        ["stream", "poll", "snapshot"] as const,
        "OrderIntent.quote_snapshot.source",
      );
      if (snap.bid !== undefined) validateDecimal(snap.bid, "OrderIntent.quote_snapshot.bid");
      if (snap.ask !== undefined) validateDecimal(snap.ask, "OrderIntent.quote_snapshot.ask");
      if (snap.mid !== undefined) validateDecimal(snap.mid, "OrderIntent.quote_snapshot.mid");
      if (snap.last !== undefined) validateDecimal(snap.last, "OrderIntent.quote_snapshot.last");
      if (snap.bid_size !== undefined)
        validateDecimal(snap.bid_size, "OrderIntent.quote_snapshot.bid_size");
      if (snap.ask_size !== undefined)
        validateDecimal(snap.ask_size, "OrderIntent.quote_snapshot.ask_size");
      if (snap.seq !== undefined) assertNumber(snap.seq, "OrderIntent.quote_snapshot.seq");
      validateUlid(oi.caps_version, "OrderIntent.caps_version");
      validateTimestamp(oi.created_ts, "OrderIntent.created_ts");
      return canonicalJson(oi);
    }

    // ── RiskVerdict ─────────────────────────────────────────────
    case "RiskVerdict": {
      const rv = value as Record<string, unknown>;
      assertObject(value, "RiskVerdict");
      validateUlid(rv.intent_id, "RiskVerdict.intent_id");
      assertString(rv.verdict, "RiskVerdict.verdict");
      assertOneOf(rv.verdict, ["allow", "deny"] as const, "RiskVerdict.verdict");
      if (rv.reasons !== undefined) {
        assertArray(rv.reasons, "RiskVerdict.reasons");
        for (let i = 0; i < rv.reasons.length; i++) {
          const reason = rv.reasons[i] as Record<string, unknown>;
          assertObject(reason, `RiskVerdict.reasons[${i}]`);
          assertString(reason.code, `RiskVerdict.reasons[${i}].code`);
          assertOneOf(
            reason.code,
            [
              "liveness",
              "price_drift",
              "balance",
              "venue_health",
              "cap_exceeded",
              "jurisdiction",
              "live_disabled",
            ] as const,
            `RiskVerdict.reasons[${i}].code`,
          );
          assertString(reason.detail, `RiskVerdict.reasons[${i}].detail`);
        }
      }
      validateTimestamp(rv.ts, "RiskVerdict.ts");
      return canonicalJson(rv);
    }

    // ── Order / Fill (same shape) ───────────────────────────────
    case "Order":
    case "Fill": {
      const o = value as Record<string, unknown>;
      assertObject(value, typ);
      validateUlid(o.order_id, `${typ}.order_id`);
      validateMarketKey(o.market, `${typ}.market`);
      assertString(o.side, `${typ}.side`);
      assertOneOf(o.side, ["buy", "sell", "buy_no", "sell_no"] as const, `${typ}.side`);
      validateDecimal(o.price, `${typ}.price`);
      validateDecimal(o.size, `${typ}.size`);
      assertObject(o.fee, `${typ}.fee`);
      const fee = o.fee as Record<string, unknown>;
      validateDecimal(fee.amount, `${typ}.fee.amount`);
      assertString(fee.currency, `${typ}.fee.currency`);
      if (fee.currency === "") throw new Error(`${typ}.fee.currency: must not be empty`);
      validateTimestamp(o.ts, `${typ}.ts`);
      assertBoolean(o.paper, `${typ}.paper`);
      // venue_ref is any JSON value — just assert it exists
      if (!("venue_ref" in o)) throw new Error(`${typ}: missing "venue_ref"`);
      return canonicalJson(o);
    }

    // ── Position ────────────────────────────────────────────────
    case "Position": {
      const p = value as Record<string, unknown>;
      assertObject(value, "Position");
      validateMarketKey(p.market, "Position.market");
      validateDecimal(p.side_exposure, "Position.side_exposure");
      validateDecimal(p.avg_price, "Position.avg_price");
      validateDecimal(p.size, "Position.size");
      assertObject(p.realized_pnl, "Position.realized_pnl");
      const rp = p.realized_pnl as Record<string, unknown>;
      validateDecimal(rp.amount, "Position.realized_pnl.amount");
      assertString(rp.currency, "Position.realized_pnl.currency");
      if (rp.currency === "") throw new Error("Position.realized_pnl.currency: must not be empty");
      assertObject(p.unrealized_pnl, "Position.unrealized_pnl");
      const up = p.unrealized_pnl as Record<string, unknown>;
      validateDecimal(up.amount, "Position.unrealized_pnl.amount");
      assertString(up.currency, "Position.unrealized_pnl.currency");
      if (up.currency === "")
        throw new Error("Position.unrealized_pnl.currency: must not be empty");
      validateTimestamp(p.ts, "Position.ts");
      return canonicalJson(p);
    }

    // ── CapsSnapshot ────────────────────────────────────────────
    case "CapsSnapshot": {
      const cs = value as Record<string, unknown>;
      assertObject(value, "CapsSnapshot");
      validateUlid(cs.version, "CapsSnapshot.version");
      assertObject(cs.per_order_max, "CapsSnapshot.per_order_max");
      const pom = cs.per_order_max as Record<string, unknown>;
      validateDecimal(pom.amount, "CapsSnapshot.per_order_max.amount");
      assertString(pom.currency, "CapsSnapshot.per_order_max.currency");
      if (pom.currency === "")
        throw new Error("CapsSnapshot.per_order_max.currency: must not be empty");
      assertObject(cs.daily_max, "CapsSnapshot.daily_max");
      const dm = cs.daily_max as Record<string, unknown>;
      validateDecimal(dm.amount, "CapsSnapshot.daily_max.amount");
      assertString(dm.currency, "CapsSnapshot.daily_max.currency");
      if (dm.currency === "") throw new Error("CapsSnapshot.daily_max.currency: must not be empty");
      if (cs.per_venue !== undefined) assertObject(cs.per_venue, "CapsSnapshot.per_venue");
      if (cs.per_kind !== undefined) assertObject(cs.per_kind, "CapsSnapshot.per_kind");
      return canonicalJson(cs);
    }

    // ── Market ──────────────────────────────────────────────────
    case "Market": {
      const m = value as Record<string, unknown>;
      assertObject(value, "Market");
      validateMarketKey(m.key, "Market.key");
      assertString(m.venue, "Market.venue");
      if (!/^[a-z0-9]+$/.test(m.venue as string) || (m.venue as string).length === 0)
        throw new Error(`Market.venue: invalid VenueId, got "${m.venue}"`);
      assertString(m.kind, "Market.kind");
      assertOneOf(
        m.kind,
        [
          "binary_contract",
          "categorical_contract",
          "scalar_contract",
          "equity",
          "option",
          "perp",
          "spot",
        ] as const,
        "Market.kind",
      );
      assertString(m.title, "Market.title");
      if (m.title === "") throw new Error("Market.title: must not be empty");
      assertString(m.description_ref, "Market.description_ref");
      if (m.description_ref === "") throw new Error("Market.description_ref: must not be empty");
      assertString(m.status, "Market.status");
      assertOneOf(m.status, ["open", "halted", "closed", "resolved"] as const, "Market.status");
      if (m.close_ts !== undefined) validateTimestamp(m.close_ts, "Market.close_ts");
      if (m.resolve_ts !== undefined) validateTimestamp(m.resolve_ts, "Market.resolve_ts");
      if (m.outcome !== undefined) assertString(m.outcome, "Market.outcome");
      assertStringArray(m.jurisdiction_flags, "Market.jurisdiction_flags");
      if (!("venue_ref" in m)) throw new Error('Market: missing "venue_ref"');
      if (!("meta" in m)) throw new Error('Market: missing "meta"');
      return canonicalJson(m);
    }

    // ── PriceSemantics ──────────────────────────────────────────
    case "PriceSemantics": {
      const ps = value as Record<string, unknown>;
      assertObject(value, "PriceSemantics");
      assertString(ps.kind, "PriceSemantics.kind");
      switch (ps.kind) {
        case "probability": {
          if (ps.tick_size === undefined) throw new Error('PriceSemantics: missing "tick_size"');
          validateDecimal(ps.tick_size, "PriceSemantics.tick_size");
          break;
        }
        case "scalar": {
          if (ps.unit === undefined) throw new Error('PriceSemantics: missing "unit"');
          assertString(ps.unit, "PriceSemantics.unit");
          if (ps.min === undefined) throw new Error('PriceSemantics: missing "min"');
          validateDecimal(ps.min, "PriceSemantics.min");
          if (ps.max === undefined) throw new Error('PriceSemantics: missing "max"');
          validateDecimal(ps.max, "PriceSemantics.max");
          break;
        }
        case "currency":
          break;
        default:
          throw new Error(`PriceSemantics: unknown kind "${ps.kind}"`);
      }
      return canonicalJson(ps);
    }

    // ── Opportunity ─────────────────────────────────────────────
    case "Opportunity": {
      const opp = value as Record<string, unknown>;
      assertObject(value, "Opportunity");
      validateUlid(opp.id, "Opportunity.id");
      assertString(opp.kind, "Opportunity.kind");
      assertOneOf(
        opp.kind,
        ["arbitrage", "value", "catalyst", "hedge"] as const,
        "Opportunity.kind",
      );
      assertArray(opp.legs, "Opportunity.legs");
      for (let i = 0; i < opp.legs.length; i++) {
        const leg = opp.legs[i] as Record<string, unknown>;
        assertObject(leg, `Opportunity.legs[${i}]`);
        validateMarketKey(leg.market, `Opportunity.legs[${i}].market`);
        assertString(leg.side, `Opportunity.legs[${i}].side`);
        assertOneOf(
          leg.side,
          ["buy", "sell", "buy_no", "sell_no"] as const,
          `Opportunity.legs[${i}].side`,
        );
        if (leg.target_price !== undefined)
          validateDecimal(leg.target_price, `Opportunity.legs[${i}].target_price`);
        if (leg.size_hint !== undefined)
          validateDecimal(leg.size_hint, `Opportunity.legs[${i}].size_hint`);
      }
      validateDecimal(opp.gross_edge, "Opportunity.gross_edge");
      assertObject(opp.edge, "Opportunity.edge");
      const edge = opp.edge as Record<string, unknown>;
      // Validate EdgeDecomposition fields
      const EDGE_FIELDS = [
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
      ] as const;
      for (const f of EDGE_FIELDS) {
        if (!(f in edge)) throw new Error(`Opportunity.edge: missing field "${f}"`);
        validateDecimal(edge[f], `Opportunity.edge.${f}`);
      }
      // Validate nested Confidence
      assertString(opp.confidence, "Opportunity.confidence");
      const cn = Number(opp.confidence);
      if (!Number.isFinite(cn) || cn < 0 || cn > 1)
        throw new Error(`Opportunity.confidence: must be in [0,1], got "${opp.confidence}"`);
      validateTimestamp(opp.detected_ts, "Opportunity.detected_ts");
      if (opp.expires_ts !== undefined) validateTimestamp(opp.expires_ts, "Opportunity.expires_ts");
      assertObject(opp.explain_ref, "Opportunity.explain_ref");
      const ref = opp.explain_ref as Record<string, unknown>;
      validateUlid(ref.object_id, "Opportunity.explain_ref.object_id");
      assertString(ref.provenance_hash, "Opportunity.explain_ref.provenance_hash");
      validateUlid(opp.trace_id, "Opportunity.trace_id");
      return canonicalJson(opp);
    }

    // ── AuditEvent ──────────────────────────────────────────────
    case "AuditEvent": {
      const ae = value as Record<string, unknown>;
      assertObject(value, "AuditEvent");
      assertNumber(ae.seq, "AuditEvent.seq");
      if (!Number.isInteger(ae.seq) || (ae.seq as number) < 0)
        throw new Error(`AuditEvent.seq: must be a non-negative integer, got ${ae.seq}`);
      assertString(ae.prev_hash, "AuditEvent.prev_hash");
      assertString(ae.hash, "AuditEvent.hash");
      if (ae.hash === "") throw new Error("AuditEvent.hash: must not be empty");
      validateTimestamp(ae.ts, "AuditEvent.ts");
      assertString(ae.actor, "AuditEvent.actor");
      if (ae.actor === "") throw new Error("AuditEvent.actor: must not be empty");
      assertString(ae.action, "AuditEvent.action");
      if (ae.action === "") throw new Error("AuditEvent.action: must not be empty");
      assertString(ae.subject, "AuditEvent.subject");
      if (ae.subject === "") throw new Error("AuditEvent.subject: must not be empty");
      assertString(ae.payload_hash, "AuditEvent.payload_hash");
      if (ae.payload_hash === "") throw new Error("AuditEvent.payload_hash: must not be empty");
      return canonicalJson(ae);
    }

    // ── ErrorEnvelope ───────────────────────────────────────────
    case "ErrorEnvelope": {
      const ee = value as Record<string, unknown>;
      assertObject(value, "ErrorEnvelope");
      assertString(ee.code, "ErrorEnvelope.code");
      assertOneOf(ee.code, Object.keys(ERR_RETRYABLE), "ErrorEnvelope.code");
      assertString(ee.message, "ErrorEnvelope.message");
      if (ee.message === "") throw new Error("ErrorEnvelope.message: must not be empty");
      assertBoolean(ee.retryable, "ErrorEnvelope.retryable");
      // Validate retryable consistency with code
      const expected = ERR_RETRYABLE[ee.code as string];
      if (ee.retryable !== expected)
        throw new Error(
          `ErrorEnvelope: retryable=${ee.retryable} contradicts code "${ee.code}" (expected ${expected})`,
        );
      validateUlid(ee.trace_id, "ErrorEnvelope.trace_id");
      if (ee.details !== undefined) assertString(ee.details, "ErrorEnvelope.details");
      return canonicalJson(ee);
    }

    default:
      throw new Error(`validateAndCanonicalize: unknown type "${typ}"`);
  }
}
