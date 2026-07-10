//! Opportunity types: OpportunityKind, Opportunity, EdgeDecomposition, BrainRef.
//! SPEC-001 opportunity & edge types. INV-1: edge math is deterministic.

use crate::decimal::{decimal_string, Confidence};
use crate::ids::{MarketKey, Ulid};
use crate::order::Side;
use crate::time::UtcTime;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

// ── Opportunity Kind ───────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpportunityKind {
    Arbitrage,
    Value,
    Catalyst,
    Hedge,
}

// ── Leg ────────────────────────────────────────────────────────────

/// One leg of a multi-leg opportunity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpportunityLeg {
    pub market: MarketKey,
    pub side: Side,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_price: Option<Decimal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_hint: Option<Decimal>,
}

// ── Brain Reference ────────────────────────────────────────────────

/// Reference to a Brain object. Full object model in SPEC-011.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrainRef {
    pub object_id: Ulid,
    pub provenance_hash: String,
}

// ── Edge Decomposition ─────────────────────────────────────────────

/// Full net-edge decomposition. All components are Decimal, all present.
/// Zero must be explicit, never defaulted (SPEC-001 golden rule).
/// `net_edge` MUST equal `gross_spread` minus sum of all other components.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EdgeDecomposition {
    #[serde(with = "decimal_string")]
    pub gross_spread: Decimal,
    #[serde(with = "decimal_string")]
    pub fees: Decimal,
    #[serde(with = "decimal_string")]
    pub slippage_est: Decimal,
    #[serde(with = "decimal_string")]
    pub funding_cost: Decimal,
    #[serde(with = "decimal_string")]
    pub gas_cost: Decimal,
    #[serde(with = "decimal_string")]
    pub bridge_cost: Decimal,
    #[serde(with = "decimal_string")]
    pub settlement_mismatch_discount: Decimal,
    #[serde(with = "decimal_string")]
    pub liquidity_haircut: Decimal,
    #[serde(with = "decimal_string")]
    pub staleness_penalty: Decimal,
    #[serde(with = "decimal_string")]
    pub confidence_penalty: Decimal,
    #[serde(with = "decimal_string")]
    pub net_edge: Decimal,
}

/// EdgeDecomposition validation error.
#[derive(Debug, thiserror::Error)]
pub enum EdgeError {
    #[error("net_edge {net_edge} != gross_spread {gross_spread} - costs {costs_sum}")]
    SumLawViolation { gross_spread: Decimal, costs_sum: Decimal, net_edge: Decimal },
}

impl EdgeDecomposition {
    /// Validate the sum law: net_edge == gross_spread - (sum of all costs).
    /// Returns Ok if valid, Err(EdgeError) if violated.
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

    /// Compute and create a valid EdgeDecomposition. Costs are provided; net_edge is computed.
    pub fn compute(gross_spread: Decimal, costs: EdgeCosts) -> Self {
        let costs_sum = costs.sum();
        let net_edge = gross_spread - costs_sum;
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
            net_edge,
        }
    }
}

/// Edge cost components (all are positive costs, subtracted from gross_spread).
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

    #[test]
    fn edge_decomposition_sum_law_valid() {
        let edge = EdgeDecomposition::compute(
            Decimal::new(100, 2), // gross_spread = 1.00
            EdgeCosts {
                fees: Decimal::new(10, 2),        // 0.10
                slippage_est: Decimal::new(5, 2), // 0.05
                funding_cost: Decimal::ZERO,
                gas_cost: Decimal::new(2, 2), // 0.02
                bridge_cost: Decimal::ZERO,
                settlement_mismatch_discount: Decimal::ZERO,
                liquidity_haircut: Decimal::new(3, 2), // 0.03
                staleness_penalty: Decimal::ZERO,
                confidence_penalty: Decimal::ZERO,
            },
        );
        assert!(edge.validate().is_ok());
        // gross 1.00 - costs 0.20 = net 0.80
        assert_eq!(edge.net_edge, Decimal::new(80, 2));
    }

    #[test]
    fn edge_decomposition_sum_law_violated() {
        let mut edge = EdgeDecomposition::compute(
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
        );
        // Corrupt the net_edge
        edge.net_edge = Decimal::new(95, 2); // should be 0.90
        assert!(edge.validate().is_err());
    }

    #[test]
    fn opportunity_kind_serde() {
        for kind in &[
            OpportunityKind::Arbitrage,
            OpportunityKind::Value,
            OpportunityKind::Catalyst,
            OpportunityKind::Hedge,
        ] {
            let json = serde_json::to_string(kind).unwrap();
            let back: OpportunityKind = serde_json::from_str(&json).unwrap();
            assert_eq!(*kind, back);
        }
    }
}
