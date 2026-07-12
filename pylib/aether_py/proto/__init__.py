"""Hand-mirrored proto message types for all gRPC service contracts (D7).

These Pydantic models mirror the proto definitions in proto/aether/ exactly.
They are the wire-format representations of all service request/response types.
"""

import re
from decimal import Decimal
from enum import StrEnum

from pydantic import BaseModel, field_validator

# Validator helpers
ULID_RE = re.compile(r"^[0-9A-HJKMNP-TV-Za-hjkmnp-tv-z]{26}$")
MARKET_KEY_RE = re.compile(r"^mkt:([a-z0-9]+):(.+)$")
VENUE_ID_RE = re.compile(r"^[a-z0-9]+$")
RFC3339_RE = re.compile(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$")


def _validate_ulid(v: str) -> str:
    if not ULID_RE.match(v):
        raise ValueError(f"Invalid ULID: {v!r}")
    return v


def _validate_market_key(v: str) -> str:
    if not MARKET_KEY_RE.match(v):
        raise ValueError(f"Invalid MarketKey: {v!r}")
    return v


def _validate_venue_id(v: str) -> str:
    if not VENUE_ID_RE.match(v):
        raise ValueError(f"Invalid VenueId: {v!r}")
    return v


def _validate_decimal_str(v: str) -> str:
    Decimal(v)
    return v


def _validate_rfc3339(v: str) -> str:
    if not RFC3339_RE.match(v):
        raise ValueError(f"Invalid RFC3339: {v!r}")
    return v


# types.proto
class ErrorCode(StrEnum):
    unspecified = "unspecified"
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


class Ulid(BaseModel):
    value: str

    @field_validator("value")
    @classmethod
    def check(cls, v: str) -> str:
        return _validate_ulid(v)


class MarketKey(BaseModel):
    value: str

    @field_validator("value")
    @classmethod
    def check(cls, v: str) -> str:
        return _validate_market_key(v)


class VenueId(BaseModel):
    value: str

    @field_validator("value")
    @classmethod
    def check(cls, v: str) -> str:
        return _validate_venue_id(v)


class Money(BaseModel):
    amount: str
    currency: str

    @field_validator("amount")
    @classmethod
    def dec(cls, v: str) -> str:
        return _validate_decimal_str(v)


class UtcTime(BaseModel):
    ts: str

    @field_validator("ts")
    @classmethod
    def check(cls, v: str) -> str:
        return _validate_rfc3339(v)


class Confidence(BaseModel):
    value: str

    @field_validator("value")
    @classmethod
    def in_range(cls, v: str) -> str:
        d = Decimal(v)
        if d < Decimal("0") or d > Decimal("1"):
            raise ValueError(f"Confidence out of [0,1]: {v!r}")
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
        return _validate_rfc3339(v)

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


# orders.proto
class Side(StrEnum):
    buy = "buy"
    sell = "sell"
    buy_no = "buy_no"
    sell_no = "sell_no"


class OrderType(StrEnum):
    limit = "limit"
    market = "market"


class TimeInForce(StrEnum):
    ioc = "ioc"
    gtc = "gtc"
    day = "day"


class SizeUnit(StrEnum):
    contracts = "contracts"
    shares = "shares"
    base = "base"
    quote = "quote"


class OriginKind(StrEnum):
    user = "user"
    alert_action = "alert_action"
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
    quote_snapshot: "Quote"
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
        return _validate_decimal_str(v)

    @field_validator("created_ts")
    @classmethod
    def valid_ts(cls, v: str) -> str:
        return _validate_rfc3339(v)


class RiskVerdict(BaseModel):
    intent_id: Ulid
    verdict: RiskVerdictStatus
    reasons: list[RiskReason] = []
    ts: str

    @field_validator("ts")
    @classmethod
    def valid_ts(cls, v: str) -> str:
        return _validate_rfc3339(v)


class Order(BaseModel):
    order_id: Ulid
    market: MarketKey
    side: Side
    price: str
    size: str
    fee: Money
    venue_ref: str
    ts: str
    paper: bool

    @field_validator("price", "size")
    @classmethod
    def dec_str(cls, v: str) -> str:
        return _validate_decimal_str(v)

    @field_validator("ts")
    @classmethod
    def valid_ts(cls, v: str) -> str:
        return _validate_rfc3339(v)


class Fill(BaseModel):
    order_id: Ulid
    market: MarketKey
    side: Side
    price: str
    size: str
    fee: Money
    venue_ref: str
    ts: str
    paper: bool

    @field_validator("price", "size")
    @classmethod
    def dec_str(cls, v: str) -> str:
        return _validate_decimal_str(v)

    @field_validator("ts")
    @classmethod
    def valid_ts(cls, v: str) -> str:
        return _validate_rfc3339(v)


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
        return _validate_decimal_str(v)

    @field_validator("ts")
    @classmethod
    def valid_ts(cls, v: str) -> str:
        return _validate_rfc3339(v)


class CapsSnapshot(BaseModel):
    version: Ulid
    per_order_max: Money
    daily_max: Money
    per_venue: dict[str, str] = {}
    per_kind: dict[str, str] = {}


# market_data.proto
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


class QuoteSource(StrEnum):
    stream = "stream"
    poll = "poll"
    snapshot = "snapshot"


class PriceSemantics(BaseModel):
    kind: str
    tick_size: str | None = None
    unit: str | None = None
    min: str | None = None
    max: str | None = None


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
    venue_ref: str
    meta: str

    @field_validator("close_ts", "resolve_ts")
    @classmethod
    def valid_ts_opt(cls, v: str | None) -> str | None:
        if v is not None:
            return _validate_rfc3339(v)
        return v


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
        return _validate_rfc3339(v)


class BookLevel(BaseModel):
    price: str
    size: str

    @field_validator("*")
    @classmethod
    def dec_str(cls, v: str) -> str:
        return _validate_decimal_str(v)


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
        return _validate_rfc3339(v)


# opportunity.proto
class OpportunityKind(StrEnum):
    arbitrage = "arbitrage"
    value = "value"
    catalyst = "catalyst"
    hedge = "hedge"


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
        return _validate_decimal_str(v)


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
        return _validate_decimal_str(v)

    @field_validator("detected_ts", "expires_ts")
    @classmethod
    def valid_ts(cls, v: str | None) -> str | None:
        if v is not None:
            return _validate_rfc3339(v)
        return v


# venue/v1/adapter.proto
class ListMarketsRequest(BaseModel):
    filter: str = ""


class GetMarketRequest(BaseModel):
    key: MarketKey


class StreamTicksRequest(BaseModel):
    keys: list[MarketKey]


class StreamBookRequest(BaseModel):
    key: MarketKey
    depth: int = 0


class CancelOrderRequest(BaseModel):
    venue_ref: str


class CancelOrderResponse(BaseModel):
    cancelled: bool


class OrderAck(BaseModel):
    venue_ref: str
    status: str


class Balance(BaseModel):
    asset: str
    free: str
    locked: str

    @field_validator("free", "locked")
    @classmethod
    def dec_str(cls, v: str) -> str:
        return _validate_decimal_str(v)


class Balances(BaseModel):
    balances: list[Balance]


class GetBalancesRequest(BaseModel):
    pass


class HealthRequest(BaseModel):
    pass


class VenueHealth(BaseModel):
    status: str
    lag_ms: int
    rate_remaining: int


# router/v1/router.proto
class RouterResult(BaseModel):
    order: Order | None = None
    verdict: RiskVerdict | None = None


class CancelRequest(BaseModel):
    order_id: Ulid


class CancelResponse(BaseModel):
    cancelled: bool


class StatusRequest(BaseModel):
    order_id: Ulid


# guardian/v1/guardian.proto
class ProposalStatus(StrEnum):
    unspecified = "unspecified"
    pending = "pending"
    auto_approved = "auto_approved"
    denied = "denied"


class TxSpec(BaseModel):
    to: str
    value: str
    data: str
    chain_id: str


class Approval(BaseModel):
    totp: str
    ts: str


class ApproveProposalRequest(BaseModel):
    id: str
    approval: Approval


class ProposalRequest(BaseModel):
    id: str


class Proposal(BaseModel):
    id: str
    status: ProposalStatus
    policy_trace: str


# brain/v1/brain.proto
class ObjectDraft(BaseModel):
    kind: str
    content: str
    source: str


class RecallRequest(BaseModel):
    query: str
    k: int = 10
    filters: str = ""


class ScoredRef(BaseModel):
    ref: BrainRef
    score: float


class RecallResponse(BaseModel):
    refs: list[ScoredRef]


class ExplainRequest(BaseModel):
    opportunity_id: Ulid


class ExplainTree(BaseModel):
    tree_json: str


# Type registry
PROTO_TYPE_REGISTRY: dict[str, type[BaseModel]] = {
    "Money": Money,
    "Ulid": Ulid,
    "MarketKey": MarketKey,
    "VenueId": VenueId,
    "UtcTime": UtcTime,
    "Confidence": Confidence,
    "AuditEvent": AuditEvent,
    "ErrorEnvelope": ErrorEnvelope,
    "Origin": Origin,
    "RiskReason": RiskReason,
    "OrderIntent": OrderIntent,
    "RiskVerdict": RiskVerdict,
    "Order": Order,
    "Fill": Fill,
    "Position": Position,
    "CapsSnapshot": CapsSnapshot,
    "PriceSemantics": PriceSemantics,
    "Market": Market,
    "Quote": Quote,
    "BookLevel": BookLevel,
    "OrderBook": OrderBook,
    "OpportunityLeg": OpportunityLeg,
    "BrainRef": BrainRef,
    "EdgeDecomposition": EdgeDecomposition,
    "Opportunity": Opportunity,
    "ListMarketsRequest": ListMarketsRequest,
    "GetMarketRequest": GetMarketRequest,
    "StreamTicksRequest": StreamTicksRequest,
    "StreamBookRequest": StreamBookRequest,
    "CancelOrderRequest": CancelOrderRequest,
    "CancelOrderResponse": CancelOrderResponse,
    "OrderAck": OrderAck,
    "Balance": Balance,
    "Balances": Balances,
    "GetBalancesRequest": GetBalancesRequest,
    "HealthRequest": HealthRequest,
    "VenueHealth": VenueHealth,
    "RouterResult": RouterResult,
    "CancelRequest": CancelRequest,
    "CancelResponse": CancelResponse,
    "StatusRequest": StatusRequest,
    "TxSpec": TxSpec,
    "Approval": Approval,
    "ApproveProposalRequest": ApproveProposalRequest,
    "ProposalRequest": ProposalRequest,
    "Proposal": Proposal,
    "ObjectDraft": ObjectDraft,
    "RecallRequest": RecallRequest,
    "ScoredRef": ScoredRef,
    "RecallResponse": RecallResponse,
    "ExplainRequest": ExplainRequest,
    "ExplainTree": ExplainTree,
}
