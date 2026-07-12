"""Complete Python domain type mirrors (all SPEC-001 types)."""

import re
from decimal import Decimal
from enum import StrEnum
from typing import Any

from pydantic import BaseModel, RootModel, field_validator, model_validator

# ── Validator helpers ──────────────────────────────────────────────────────────

ULID_RE = re.compile(r"^[0-9A-HJKMNP-TV-Za-hjkmnp-tv-z]{26}$")
MARKET_KEY_RE = re.compile(r"^mkt:([a-z0-9]+):(.+)$")
VENUE_ID_RE = re.compile(r"^[a-z0-9]+$")
RFC3339_RE = re.compile(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$")


def _decimal_str(v: str) -> str:
    Decimal(v)
    return v


def _ulid(v: str) -> str:
    if not ULID_RE.match(v):
        raise ValueError(f"Invalid ULID: {v!r}")
    return v


def _market_key(v: str) -> str:
    m = MARKET_KEY_RE.match(v)
    if not m or not m.group(1) or not m.group(2):
        raise ValueError(f"Invalid MarketKey: {v!r}")
    return v


def _venue_id(v: str) -> str:
    if not VENUE_ID_RE.match(v):
        raise ValueError(f"Invalid VenueId: {v!r}")
    return v


def _rfc3339(v: str) -> str:
    if not RFC3339_RE.match(v):
        raise ValueError(f"Invalid RFC3339: {v!r}")
    return v


# ── Enums ─────────────────────────────────────────────────────────────────----


class Side(StrEnum):
    buy = "buy"
    sell = "sell"
    buy_no = "buy_no"
    sell_no = "sell_no"


class OrderType(StrEnum):
    limit = "limit"
    market = "market"


class SizeUnit(StrEnum):
    contracts = "contracts"
    shares = "shares"
    base = "base"
    quote = "quote"


class TimeInForce(StrEnum):
    ioc = "ioc"
    gtc = "gtc"
    day = "day"


class OriginKind(StrEnum):
    human = "human"
    agent = "agent"
    automation = "automation"


class RiskVerdictStatus(StrEnum):
    allow = "allow"
    deny = "deny"


class RiskReasonCode(StrEnum):
    liveness = "liveness"
    price_drift = "price_drift"
    balance = "balance"
    venue_health = "venue_health"
    cap_exceeded = "cap_exceeded"
    jurisdiction = "jurisdiction"
    live_disabled = "live_disabled"


class QuoteSource(StrEnum):
    stream = "stream"
    poll = "poll"
    snapshot = "snapshot"


class InstrumentKind(StrEnum):
    binary_contract = "binary_contract"
    categorical_contract = "categorical_contract"
    scalar_contract = "scalar_contract"
    equity = "equity"
    option = "option"
    perp = "perp"
    spot = "spot"


class MarketStatus(StrEnum):
    open = "open"
    halted = "halted"
    closed = "closed"
    resolved = "resolved"


class OpportunityKind(StrEnum):
    arbitrage = "arbitrage"
    value = "value"
    catalyst = "catalyst"
    hedge = "hedge"


class ErrorCode(StrEnum):
    invalid_argument = "invalid_argument"
    unauthenticated = "unauthenticated"
    permission_denied = "permission_denied"
    not_found = "not_found"
    failed_precondition = "failed_precondition"
    unavailable = "unavailable"
    deadline_exceeded = "deadline_exceeded"
    quarantined = "quarantined"
    internal = "internal"

    def is_retryable(self) -> bool:
        return self in (ErrorCode.unavailable, ErrorCode.deadline_exceeded)


# ── Scalar / newtype validators (RootModel) ──────────────────────────────────


class Ulid(RootModel[str]):
    """26-char Crockford base32 ULID (no I/L/O/U)."""

    root: str

    @field_validator("root")
    @classmethod
    def check(cls, v: str) -> str:
        return _ulid(v)


class VenueId(RootModel[str]):
    """Lowercase alphanumeric, non-empty."""

    root: str

    @field_validator("root")
    @classmethod
    def check(cls, v: str) -> str:
        return _venue_id(v)


class MarketKey(RootModel[str]):
    """Format: mkt:{venue}:{native_id}."""

    root: str

    @field_validator("root")
    @classmethod
    def check(cls, v: str) -> str:
        return _market_key(v)


class Confidence(RootModel[str]):
    """Decimal string in [0, 1]."""

    root: str

    @field_validator("root")
    @classmethod
    def in_range(cls, v: str) -> str:
        d = Decimal(v)
        if d < Decimal("0") or d > Decimal("1"):
            raise ValueError(f"Confidence out of [0,1]: {v!r}")
        return v


# ── Domain types ──────────────────────────────────────────────────────────────


class Money(BaseModel):
    amount: str
    currency: str

    @field_validator("amount")
    @classmethod
    def dec_str(cls, v: str) -> str:
        return _decimal_str(v)

    @field_validator("currency")
    @classmethod
    def non_empty(cls, v: str) -> str:
        if not v:
            raise ValueError("currency must be non-empty")
        return v


class EdgeDecomposition(BaseModel):
    gross_spread: str
    fees: str
    slippage_est: str
    funding_cost: str
    gas_cost: str
    bridge_cost: str
    settlement_mismatch_discount: str
    liquidity_haircut: str
    staleness_penalty: str
    confidence_penalty: str
    net_edge: str

    @field_validator("*")
    @classmethod
    def dec_str(cls, v: str) -> str:
        return _decimal_str(v)

    @model_validator(mode="after")
    def sum_law(self):
        costs = sum(
            Decimal(getattr(self, f))
            for f in [
                "fees",
                "slippage_est",
                "funding_cost",
                "gas_cost",
                "bridge_cost",
                "settlement_mismatch_discount",
                "liquidity_haircut",
                "staleness_penalty",
                "confidence_penalty",
            ]
        )
        if Decimal(self.net_edge) != Decimal(self.gross_spread) - costs:
            raise ValueError(
                f"net_edge {self.net_edge} != gross_spread {self.gross_spread} - costs {costs}"
            )
        return self


class BookLevel(BaseModel):
    price: str
    size: str

    @field_validator("*")
    @classmethod
    def dec_str(cls, v: str) -> str:
        return _decimal_str(v)


class Quote(BaseModel):
    market: MarketKey
    bid: str | None = None
    ask: str | None = None
    mid: str | None = None
    last: str | None = None
    bid_size: str | None = None
    ask_size: str | None = None
    ts: str
    source: QuoteSource
    seq: int | None = None

    @field_validator("bid", "ask", "mid", "last", "bid_size", "ask_size")
    @classmethod
    def dec_str_opt(cls, v: str | None) -> str | None:
        if v is not None:
            Decimal(v)
        return v

    @field_validator("ts")
    @classmethod
    def valid_ts(cls, v: str) -> str:
        return _rfc3339(v)


class OrderBook(BaseModel):
    market: MarketKey
    bids: list[BookLevel]
    asks: list[BookLevel]
    depth: int
    ts: str
    seq: int | None = None

    @field_validator("ts")
    @classmethod
    def valid_ts(cls, v: str) -> str:
        return _rfc3339(v)

    @model_validator(mode="after")
    def ordering(self):
        for i in range(len(self.bids) - 1):
            if Decimal(self.bids[i].price) <= Decimal(self.bids[i + 1].price):
                raise ValueError("bids not descending")
        for i in range(len(self.asks) - 1):
            if Decimal(self.asks[i].price) >= Decimal(self.asks[i + 1].price):
                raise ValueError("asks not ascending")
        return self


class Origin(BaseModel):
    kind: OriginKind
    tier: int
    actor_id: Ulid

    @field_validator("tier")
    @classmethod
    def tier_range(cls, v: int) -> int:
        if v < 1 or v > 5:
            raise ValueError(f"origin tier must be 1..=5, got {v}")
        return v


class RiskReason(BaseModel):
    code: RiskReasonCode
    detail: str


class OrderIntent(BaseModel):
    id: Ulid
    market: MarketKey
    side: Side
    order_type: OrderType
    limit_price: str | None = None
    size: str
    size_unit: SizeUnit
    tif: TimeInForce
    paper: bool
    origin: Origin
    quote_snapshot: Quote
    caps_version: Ulid
    created_ts: str

    @field_validator("limit_price")
    @classmethod
    def dec_str_opt(cls, v: str | None) -> str | None:
        if v is not None:
            Decimal(v)
        return v

    @field_validator("size")
    @classmethod
    def dec_str(cls, v: str) -> str:
        return _decimal_str(v)

    @field_validator("created_ts")
    @classmethod
    def valid_ts(cls, v: str) -> str:
        return _rfc3339(v)


class RiskVerdict(BaseModel):
    intent_id: Ulid
    verdict: RiskVerdictStatus
    reasons: list[RiskReason] = []
    ts: str

    @field_validator("ts")
    @classmethod
    def valid_ts(cls, v: str) -> str:
        return _rfc3339(v)


class Order(BaseModel):
    order_id: Ulid
    market: MarketKey
    side: Side
    price: str
    size: str
    fee: Money
    venue_ref: dict[str, Any]
    ts: str
    paper: bool

    @field_validator("price", "size")
    @classmethod
    def dec_str(cls, v: str) -> str:
        return _decimal_str(v)

    @field_validator("ts")
    @classmethod
    def valid_ts(cls, v: str) -> str:
        return _rfc3339(v)


class Fill(BaseModel):
    order_id: Ulid
    market: MarketKey
    side: Side
    price: str
    size: str
    fee: Money
    venue_ref: dict[str, Any]
    ts: str
    paper: bool

    @field_validator("price", "size")
    @classmethod
    def dec_str(cls, v: str) -> str:
        return _decimal_str(v)

    @field_validator("ts")
    @classmethod
    def valid_ts(cls, v: str) -> str:
        return _rfc3339(v)


class Position(BaseModel):
    market: MarketKey
    side_exposure: str
    avg_price: str
    size: str
    realized_pnl: Money
    unrealized_pnl: Money
    ts: str

    @field_validator("side_exposure", "avg_price", "size")
    @classmethod
    def dec_str(cls, v: str) -> str:
        return _decimal_str(v)

    @field_validator("ts")
    @classmethod
    def valid_ts(cls, v: str) -> str:
        return _rfc3339(v)


class CapsSnapshot(BaseModel):
    version: Ulid
    per_order_max: Money
    daily_max: Money
    per_venue: dict[str, Any] = {}
    per_kind: dict[str, Any] = {}


class Market(BaseModel):
    key: MarketKey
    venue: VenueId
    kind: InstrumentKind
    title: str
    description_ref: str
    status: MarketStatus
    close_ts: str | None = None
    resolve_ts: str | None = None
    outcome: str | None = None
    jurisdiction_flags: list[str]
    venue_ref: dict[str, Any]
    meta: dict[str, Any]

    @field_validator("close_ts", "resolve_ts")
    @classmethod
    def valid_ts_opt(cls, v: str | None) -> str | None:
        if v is not None:
            return _rfc3339(v)
        return v


class PriceSemantics(BaseModel):
    kind: str
    tick_size: str | None = None
    unit: str | None = None
    min: str | None = None
    max: str | None = None

    @model_validator(mode="after")
    def validate_kind(self) -> "PriceSemantics":
        if self.kind == "probability":
            if self.tick_size is None:
                raise ValueError("tick_size required for kind=probability")
            Decimal(self.tick_size)
        elif self.kind == "scalar":
            if None in (self.unit, self.min, self.max):
                raise ValueError("unit/min/max required for kind=scalar")
            assert self.min is not None and self.max is not None  # mypy narrowing
            Decimal(self.min)
            Decimal(self.max)
        elif self.kind == "currency":
            pass
        else:
            raise ValueError(f"unknown PriceSemantics kind: {self.kind!r}")
        return self


class OpportunityLeg(BaseModel):
    market: MarketKey
    side: Side
    target_price: str | None = None
    size_hint: str | None = None

    @field_validator("target_price", "size_hint")
    @classmethod
    def dec_str_opt(cls, v: str | None) -> str | None:
        if v is not None:
            Decimal(v)
        return v


class BrainRef(BaseModel):
    object_id: Ulid
    provenance_hash: str


class Opportunity(BaseModel):
    id: Ulid
    kind: OpportunityKind
    legs: list[OpportunityLeg]
    gross_edge: str
    edge: EdgeDecomposition
    confidence: Confidence
    detected_ts: str
    expires_ts: str | None = None
    explain_ref: BrainRef
    trace_id: Ulid

    @field_validator("gross_edge")
    @classmethod
    def dec_str(cls, v: str) -> str:
        return _decimal_str(v)

    @field_validator("detected_ts", "expires_ts")
    @classmethod
    def valid_ts(cls, v: str | None) -> str | None:
        if v is not None:
            return _rfc3339(v)
        return v


class AuditEvent(BaseModel):
    seq: int
    prev_hash: str
    hash: str
    ts: str
    actor: str
    action: str
    subject: str
    payload_hash: str

    @field_validator("ts")
    @classmethod
    def valid_ts(cls, v: str) -> str:
        return _rfc3339(v)

    @field_validator("seq")
    @classmethod
    def non_negative(cls, v: int) -> int:
        if v < 0:
            raise ValueError(f"seq must be >= 0, got {v}")
        return v

    @field_validator("hash", "payload_hash")
    @classmethod
    def non_empty(cls, v: str) -> str:
        if not v:
            raise ValueError("hash field must be non-empty")
        return v


class ErrorEnvelope(BaseModel):
    code: ErrorCode
    message: str
    retryable: bool
    trace_id: Ulid
    details: str | None = None

    @model_validator(mode="after")
    def retryable_consistent(self):
        expected = self.code.is_retryable()
        if self.retryable != expected:
            raise ValueError(f"retryable={self.retryable} contradicts code={self.code}")
        return self


# ── Type registry for golden-test dispatch ────────────────────────────────────

TYPE_REGISTRY: dict[str, type[BaseModel]] = {
    "Money": Money,
    "Confidence": Confidence,
    "MarketKey": MarketKey,
    "Ulid": Ulid,
    "VenueId": VenueId,
    "EdgeDecomposition": EdgeDecomposition,
    "Quote": Quote,
    "OrderBook": OrderBook,
    "OrderIntent": OrderIntent,
    "RiskVerdict": RiskVerdict,
    "Order": Order,
    "Fill": Fill,
    "Position": Position,
    "CapsSnapshot": CapsSnapshot,
    "Market": Market,
    "PriceSemantics": PriceSemantics,
    "Opportunity": Opportunity,
    "AuditEvent": AuditEvent,
    "ErrorEnvelope": ErrorEnvelope,
}
