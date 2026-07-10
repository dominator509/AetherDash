"""AETHER Terminal — Python domain type mirrors.
mirrors: crates/aether-core (SPEC-001 types)
"""

from decimal import Decimal

from pydantic import BaseModel, field_validator


class Money(BaseModel):
    """mirrors: aether.core.v1.Money"""

    amount: str  # decimal string
    currency: str  # ISO-4217 or USDC|USDT|ETH|...


class EdgeDecomposition(BaseModel):
    """mirrors: aether.core.v1.EdgeDecomposition"""

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

    @field_validator("*", mode="before")
    @classmethod
    def ensure_decimal_string(cls, v: str) -> str:
        Decimal(v)  # validate parseable
        return v


class Confidence(BaseModel):
    """mirrors: aether.core.v1.Confidence"""

    value: str  # decimal string, 0..=1

    @field_validator("value")
    @classmethod
    def validate_range(cls, v: str) -> str:
        d = Decimal(v)
        if d < 0 or d > 1:
            raise ValueError(f"confidence must be in [0, 1], got {v}")
        return v


# Import canonical serialization
