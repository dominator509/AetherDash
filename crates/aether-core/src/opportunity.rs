//! Opportunity types: OpportunityKind, Opportunity, EdgeDecomposition, BrainRef.
//! SPEC-001 opportunity & edge types. INV-1: edge math is deterministic.

use crate::decimal::{decimal_option_string, decimal_string, Confidence};
use crate::ids::{MarketKey, Ulid};
use crate::order::Side;
use crate::time::UtcTime;
use rust_decimal::Decimal;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpportunityKind {
    Arbitrage,
    Value,
    Catalyst,
    Hedge,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpportunityLeg {
    pub market: MarketKey,
    pub side: Side,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "decimal_option_string")]
    pub target_price: Option<Decimal>,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "decimal_option_string")]
    pub size_hint: Option<Decimal>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrainRef {
    pub object_id: Ulid,
    pub provenance_hash: String,
}

// ── Edge Decomposition ─────────────────────────────────────────────

/// Full net-edge decomposition. All components Decimal, all present.
/// Zero must be explicit, never defaulted. Fields are private to
/// prevent mutation after validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EdgeDecomposition {
    gross_spread: Decimal,
    fees: Decimal,
    slippage_est: Decimal,
    funding_cost: Decimal,
    gas_cost: Decimal,
    bridge_cost: Decimal,
    settlement_mismatch_discount: Decimal,
    liquidity_haircut: Decimal,
    staleness_penalty: Decimal,
    confidence_penalty: Decimal,
    net_edge: Decimal,
}

impl EdgeDecomposition {
    pub fn gross_spread(&self) -> Decimal {
        self.gross_spread
    }
    pub fn fees(&self) -> Decimal {
        self.fees
    }
    pub fn slippage_est(&self) -> Decimal {
        self.slippage_est
    }
    pub fn funding_cost(&self) -> Decimal {
        self.funding_cost
    }
    pub fn gas_cost(&self) -> Decimal {
        self.gas_cost
    }
    pub fn bridge_cost(&self) -> Decimal {
        self.bridge_cost
    }
    pub fn settlement_mismatch_discount(&self) -> Decimal {
        self.settlement_mismatch_discount
    }
    pub fn liquidity_haircut(&self) -> Decimal {
        self.liquidity_haircut
    }
    pub fn staleness_penalty(&self) -> Decimal {
        self.staleness_penalty
    }
    pub fn confidence_penalty(&self) -> Decimal {
        self.confidence_penalty
    }
    pub fn net_edge(&self) -> Decimal {
        self.net_edge
    }

    pub fn validate(&self) -> Result<(), EdgeError> {
        let costs_sum = self.fees
            + self.slippage_est
            + self.funding_cost
            + self.gas_cost
            + self.bridge_cost
            + self.settlement_mismatch_discount
            + self.liquidity_haircut
            + self.staleness_penalty
            + self.confidence_penalty;
        let expected = self.gross_spread - costs_sum;
        if self.net_edge != expected {
            return Err(EdgeError::SumLawViolation {
                gross_spread: self.gross_spread,
                costs_sum,
                net_edge: self.net_edge,
            });
        }
        Ok(())
    }

    pub fn compute(gross_spread: Decimal, costs: EdgeCosts) -> Self {
        let costs_sum = costs.sum();
        Self {
            gross_spread,
            fees: costs.fees,
            slippage_est: costs.slippage_est,
            funding_cost: costs.funding_cost,
            gas_cost: costs.gas_cost,
            bridge_cost: costs.bridge_cost,
            settlement_mismatch_discount: costs.settlement_mismatch_discount,
            liquidity_haircut: costs.liquidity_haircut,
            staleness_penalty: costs.staleness_penalty,
            confidence_penalty: costs.confidence_penalty,
            net_edge: gross_spread - costs_sum,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EdgeError {
    #[error("net_edge {net_edge} != gross_spread {gross_spread} - costs {costs_sum}")]
    SumLawViolation { gross_spread: Decimal, costs_sum: Decimal, net_edge: Decimal },
}

/// Edge cost components. Fields are pub for ergonomic construction in tests and generators.
/// The sum-law invariant is enforced by EdgeDecomposition::validate(), not here.
pub struct EdgeCosts {
    pub fees: Decimal,
    pub slippage_est: Decimal,
    pub funding_cost: Decimal,
    pub gas_cost: Decimal,
    pub bridge_cost: Decimal,
    pub settlement_mismatch_discount: Decimal,
    pub liquidity_haircut: Decimal,
    pub staleness_penalty: Decimal,
    pub confidence_penalty: Decimal,
}

impl EdgeCosts {
    pub fn zero() -> Self {
        Self {
            fees: Decimal::ZERO,
            slippage_est: Decimal::ZERO,
            funding_cost: Decimal::ZERO,
            gas_cost: Decimal::ZERO,
            bridge_cost: Decimal::ZERO,
            settlement_mismatch_discount: Decimal::ZERO,
            liquidity_haircut: Decimal::ZERO,
            staleness_penalty: Decimal::ZERO,
            confidence_penalty: Decimal::ZERO,
        }
    }

    pub fn sum(&self) -> Decimal {
        self.fees
            + self.slippage_est
            + self.funding_cost
            + self.gas_cost
            + self.bridge_cost
            + self.settlement_mismatch_discount
            + self.liquidity_haircut
            + self.staleness_penalty
            + self.confidence_penalty
    }
}

// ── Custom Serialize/Deserialize for EdgeDecomposition ─────────────

#[derive(Serialize, Deserialize)]
struct EdgeDecompositionWire {
    #[serde(with = "decimal_string")]
    gross_spread: Decimal,
    #[serde(with = "decimal_string")]
    fees: Decimal,
    #[serde(with = "decimal_string")]
    slippage_est: Decimal,
    #[serde(with = "decimal_string")]
    funding_cost: Decimal,
    #[serde(with = "decimal_string")]
    gas_cost: Decimal,
    #[serde(with = "decimal_string")]
    bridge_cost: Decimal,
    #[serde(with = "decimal_string")]
    settlement_mismatch_discount: Decimal,
    #[serde(with = "decimal_string")]
    liquidity_haircut: Decimal,
    #[serde(with = "decimal_string")]
    staleness_penalty: Decimal,
    #[serde(with = "decimal_string")]
    confidence_penalty: Decimal,
    #[serde(with = "decimal_string")]
    net_edge: Decimal,
}

impl Serialize for EdgeDecomposition {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        EdgeDecompositionWire {
            gross_spread: self.gross_spread,
            fees: self.fees,
            slippage_est: self.slippage_est,
            funding_cost: self.funding_cost,
            gas_cost: self.gas_cost,
            bridge_cost: self.bridge_cost,
            settlement_mismatch_discount: self.settlement_mismatch_discount,
            liquidity_haircut: self.liquidity_haircut,
            staleness_penalty: self.staleness_penalty,
            confidence_penalty: self.confidence_penalty,
            net_edge: self.net_edge,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for EdgeDecomposition {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = EdgeDecompositionWire::deserialize(deserializer)?;
        let edge = Self {
            gross_spread: wire.gross_spread,
            fees: wire.fees,
            slippage_est: wire.slippage_est,
            funding_cost: wire.funding_cost,
            gas_cost: wire.gas_cost,
            bridge_cost: wire.bridge_cost,
            settlement_mismatch_discount: wire.settlement_mismatch_discount,
            liquidity_haircut: wire.liquidity_haircut,
            staleness_penalty: wire.staleness_penalty,
            confidence_penalty: wire.confidence_penalty,
            net_edge: wire.net_edge,
        };
        edge.validate().map_err(serde::de::Error::custom)?;
        Ok(edge)
    }
}

// ── Display for EdgeDecomposition ──────────────────────────────────

#[cfg(test)]
impl EdgeDecomposition {
    /// Test-only constructor for arbitrary component values (including invalid sum-law).
    #[allow(clippy::too_many_arguments)]
    pub fn from_raw_components(
        gross_spread: Decimal,
        fees: Decimal,
        slippage_est: Decimal,
        funding_cost: Decimal,
        gas_cost: Decimal,
        bridge_cost: Decimal,
        settlement_mismatch_discount: Decimal,
        liquidity_haircut: Decimal,
        staleness_penalty: Decimal,
        confidence_penalty: Decimal,
        net_edge: Decimal,
    ) -> Self {
        Self {
            gross_spread,
            fees,
            slippage_est,
            funding_cost,
            gas_cost,
            bridge_cost,
            settlement_mismatch_discount,
            liquidity_haircut,
            staleness_penalty,
            confidence_penalty,
            net_edge,
        }
    }
}



impl fmt::Display for EdgeDecomposition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "EdgeDecomposition(gross={}, net={})", self.gross_spread, self.net_edge)
    }
}

// ── Opportunity ────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Opportunity {
    pub id: Ulid,
    pub kind: OpportunityKind,
    pub legs: Vec<OpportunityLeg>,
    #[serde(with = "decimal_string")]
    pub gross_edge: Decimal,
    pub edge: EdgeDecomposition,
    pub confidence: Confidence,
    pub detected_ts: UtcTime,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_ts: Option<UtcTime>,
    pub explain_ref: BrainRef,
    pub trace_id: Ulid,
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// Strategy for any Decimal value: negative, positive, zero, high-precision.
    fn arb_decimal_any() -> impl Strategy<Value = Decimal> {
        prop::num::i64::ANY.prop_map(|n| Decimal::new(n, 2))
    }

    proptest! {
        #[test]
        fn edge_decomposition_sum_law_property(
            gross in arb_decimal_any(),
            fees in arb_decimal_any(),
            slippage in arb_decimal_any(),
            funding in arb_decimal_any(),
            gas in arb_decimal_any(),
            bridge in arb_decimal_any(),
            settlement in arb_decimal_any(),
            haircut in arb_decimal_any(),
            staleness in arb_decimal_any(),
            confidence_pen in arb_decimal_any(),
            valid in proptest::bool::ANY,
        ) {
            let costs = EdgeCosts {
                fees,
                slippage_est: slippage,
                funding_cost: funding,
                gas_cost: gas,
                bridge_cost: bridge,
                settlement_mismatch_discount: settlement,
                liquidity_haircut: haircut,
                staleness_penalty: staleness,
                confidence_penalty: confidence_pen,
            };

            let costs_sum = costs.sum();

            if valid {
                // compute() always produces a correct net_edge
                let edge = EdgeDecomposition::compute(gross, costs);
                prop_assert!(edge.validate().is_ok(),
                    "compute must always produce a valid edge: gross={gross}, costs_sum={costs_sum}");
                prop_assert_eq!(edge.net_edge(), gross - costs_sum,
                    "net_edge mismatch for compute");
            } else {
                // Build an edge with net_edge that violates the sum law (off by delta)
                let delta = Decimal::new(1, 2); // 0.01
                let net_edge = gross - costs_sum + delta;
                let edge = EdgeDecomposition::from_raw_components(
                    gross, fees, slippage, funding, gas,
                    bridge, settlement, haircut, staleness, confidence_pen,
                    net_edge,
                );
                prop_assert!(edge.validate().is_err(),
                    "net_edge {net_edge} != gross {gross} - costs {costs_sum} (delta={delta}) must fail validation");
            }
        }
    }

    #[test]
    fn edge_decomposition_sum_law_valid() {
        let edge = EdgeDecomposition::compute(
            Decimal::new(100, 2),
            EdgeCosts {
                fees: Decimal::new(10, 2),
                slippage_est: Decimal::new(5, 2),
                funding_cost: Decimal::ZERO,
                gas_cost: Decimal::new(2, 2),
                bridge_cost: Decimal::ZERO,
                settlement_mismatch_discount: Decimal::ZERO,
                liquidity_haircut: Decimal::new(3, 2),
                staleness_penalty: Decimal::ZERO,
                confidence_penalty: Decimal::ZERO,
            },
        );
        assert!(edge.validate().is_ok());
        assert_eq!(edge.net_edge(), Decimal::new(80, 2));
    }

    #[test]
    fn edge_decomposition_deserialize_rejects_sum_violation() {
        // gross 1.00 minus costs 0.10 = net should be 0.90, but we say 0.95
        let json = r#"{"gross_spread":"1.00","fees":"0.10","slippage_est":"0","funding_cost":"0","gas_cost":"0","bridge_cost":"0","settlement_mismatch_discount":"0","liquidity_haircut":"0","staleness_penalty":"0","confidence_penalty":"0","net_edge":"0.95"}"#;
        let result: Result<EdgeDecomposition, _> = serde_json::from_str(json);
        assert!(result.is_err(), "invalid sum law should be rejected on deserialize");
    }

    #[test]
    fn opportunity_serde_round_trip() {
        let opp = Opportunity {
            id: Ulid::new(),
            kind: OpportunityKind::Arbitrage,
            legs: vec![],
            gross_edge: Decimal::new(100, 2),
            edge: EdgeDecomposition::compute(
                Decimal::new(100, 2),
                EdgeCosts {
                    fees: Decimal::new(10, 2),
                    slippage_est: Decimal::ZERO,
                    funding_cost: Decimal::ZERO,
                    gas_cost: Decimal::ZERO,
                    bridge_cost: Decimal::ZERO,
                    settlement_mismatch_discount: Decimal::ZERO,
                    liquidity_haircut: Decimal::ZERO,
                    staleness_penalty: Decimal::ZERO,
                    confidence_penalty: Decimal::ZERO,
                },
            ),
            confidence: Confidence::new(Decimal::new(8, 1)).unwrap(),
            detected_ts: UtcTime::from_unix_millis(1752152096789).unwrap(),
            expires_ts: None,
            explain_ref: BrainRef { object_id: Ulid::new(), provenance_hash: "abc123".into() },
            trace_id: Ulid::new(),
        };
        let json = serde_json::to_string(&opp).unwrap();
        let back: Opportunity = serde_json::from_str(&json).unwrap();
        assert_eq!(opp.id, back.id);
    }
}
