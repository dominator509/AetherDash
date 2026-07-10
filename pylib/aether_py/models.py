"""Complete Python domain type mirrors (all 17 SPEC-001 types)."""

from decimal import Decimal

from pydantic import BaseModel, field_validator, model_validator


class Money(BaseModel):
    amount: str
    currency: str

    @field_validator("amount")
    @classmethod
    def dec_str(cls, v: str) -> str:
        Decimal(v)
        return v


class Confidence(BaseModel):
    value: str

    @field_validator("value")
    @classmethod
    def in_range(cls, v: str) -> str:
        d = Decimal(v)
        if d < Decimal("0") or d > Decimal("1"):
            raise ValueError(f"confidence [0,1], got {v}")
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
        Decimal(v)
        return v

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


class Quote(BaseModel):
    market: str
    bid: str | None = None
    ask: str | None = None
    mid: str | None = None
    last: str | None = None
    bid_size: str | None = None
    ask_size: str | None = None
    ts: str
    source: str
    seq: int | None = None


class BookLevel(BaseModel):
    price: str
    size: str


class OrderBook(BaseModel):
    market: str
    bids: list[BookLevel] = []
    asks: list[BookLevel] = []
    depth: int
    ts: str
    seq: int | None = None

    @model_validator(mode="after")
    def ordering(self):
        for i in range(len(self.bids) - 1):
            if Decimal(self.bids[i].price) <= Decimal(self.bids[i + 1].price):
                raise ValueError("bids not descending")
        for i in range(len(self.asks) - 1):
            if Decimal(self.asks[i].price) >= Decimal(self.asks[i + 1].price):
                raise ValueError("asks not ascending")
        return self


class OrderIntent(BaseModel):
    id: str
    market: str
    side: str
    order_type: str
    limit_price: str | None = None
    size: str
    size_unit: str
    tif: str
    paper: bool
    origin: dict
    quote_snapshot: dict
    caps_version: str
    created_ts: str


class RiskVerdict(BaseModel):
    intent_id: str
    verdict: str
    reasons: list[dict] = []
    ts: str


class Order(BaseModel):
    order_id: str
    market: str
    side: str
    price: str
    size: str
    fee: dict
    venue_ref: dict
    ts: str
    paper: bool


class Fill(BaseModel):
    order_id: str
    market: str
    side: str
    price: str
    size: str
    fee: dict
    venue_ref: dict
    ts: str
    paper: bool


class Position(BaseModel):
    market: str
    side_exposure: str
    avg_price: str
    size: str
    realized_pnl: dict
    unrealized_pnl: dict
    ts: str


class CapsSnapshot(BaseModel):
    version: str
    per_order_max: dict
    daily_max: dict
    per_venue: dict = {}
    per_kind: dict = {}


class Market(BaseModel):
    key: str
    venue: str
    kind: str
    title: str
    description_ref: str
    status: str
    close_ts: str | None = None
    resolve_ts: str | None = None
    outcome: str | None = None
    jurisdiction_flags: list[str] = []
    venue_ref: dict
    meta: dict


class PriceSemantics(BaseModel):
    kind: str
    tick_size: str | None = None
    unit: str | None = None
    min: str | None = None
    max: str | None = None


class Opportunity(BaseModel):
    id: str
    kind: str
    legs: list[dict]
    gross_edge: str
    edge: dict
    confidence: str
    detected_ts: str
    expires_ts: str | None = None
    explain_ref: dict
    trace_id: str


class AuditEvent(BaseModel):
    seq: int
    prev_hash: str
    hash: str
    ts: str
    actor: str
    action: str
    subject: str
    payload_hash: str


class ErrorEnvelope(BaseModel):
    code: str
    message: str
    retryable: bool
    trace_id: str
    details: str | None = None

    @model_validator(mode="after")
    def retryable_consistent(self):
        expected = self.code in {"unavailable", "deadline_exceeded"}
        if self.retryable != expected:
            raise ValueError(f"retryable={self.retryable} contradicts code={self.code}")
        return self


TYPE_REGISTRY: dict[str, type[BaseModel]] = {
    "Money": Money,
    "Confidence": Confidence,
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
