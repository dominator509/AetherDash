//! P&L Attribution: closes opportunity chains with predicted vs realized P&L.
//! SPEC-012: every closed chain MUST have an attribution row.

use aether_core::ids::Ulid;
use aether_core::time::UtcTime;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Attribution record — predicted vs realized per component.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attribution {
    pub opportunity_id: Ulid,
    pub predicted_gross_spread: Decimal,
    pub realized_gross_spread: Decimal,
    pub predicted_fees: Decimal,
    pub realized_fees: Decimal,
    pub predicted_slippage: Decimal,
    pub realized_slippage: Decimal,
    pub predicted_funding: Decimal,
    pub realized_funding: Decimal,
    pub net_predicted: Decimal,
    pub net_realized: Decimal,
    pub divergence: Decimal,
    pub closed_ts: UtcTime,
}

#[derive(Error, Debug)]
pub enum AttributionError {
    #[error("insufficient data for attribution")]
    InsufficientData,
    #[error("chain already has attribution")]
    AlreadyAttributed,
}

/// Compute attribution by comparing predicted vs realized components.
pub fn compute_attribution(
    opportunity_id: Ulid,
    predicted: &AttributionInput,
    realized: &AttributionInput,
) -> Result<Attribution, AttributionError> {
    let net_pred = predicted.net();
    let net_real = realized.net();
    Ok(Attribution {
        opportunity_id,
        predicted_gross_spread: predicted.gross_spread,
        realized_gross_spread: realized.gross_spread,
        predicted_fees: predicted.fees,
        realized_fees: realized.fees,
        predicted_slippage: predicted.slippage,
        realized_slippage: realized.slippage,
        predicted_funding: predicted.funding,
        realized_funding: realized.funding,
        net_predicted: net_pred,
        net_realized: net_real,
        divergence: net_pred - net_real,
        closed_ts: UtcTime::now(),
    })
}

/// Input values for attribution computation.
#[derive(Debug, Clone, Default)]
pub struct AttributionInput {
    pub gross_spread: Decimal,
    pub fees: Decimal,
    pub slippage: Decimal,
    pub funding: Decimal,
}

impl AttributionInput {
    pub fn net(&self) -> Decimal {
        self.gross_spread - self.fees - self.slippage - self.funding
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attribution_divergence_is_predicted_minus_realized() {
        let predicted = AttributionInput {
            gross_spread: Decimal::new(10, 2), // 0.10
            fees: Decimal::new(2, 2),          // 0.02
            slippage: Decimal::new(1, 2),      // 0.01
            funding: Decimal::ZERO,
        };
        let realized = AttributionInput {
            gross_spread: Decimal::new(9, 2), // 0.09
            fees: Decimal::new(2, 2),         // 0.02
            slippage: Decimal::new(2, 2),     // 0.02
            funding: Decimal::ZERO,
        };
        let attr = compute_attribution(Ulid::new(), &predicted, &realized).unwrap();
        assert_eq!(attr.net_predicted, Decimal::new(7, 2)); // 0.10 - 0.02 - 0.01 = 0.07
        assert_eq!(attr.net_realized, Decimal::new(5, 2)); // 0.09 - 0.02 - 0.02 = 0.05
        assert_eq!(attr.divergence, Decimal::new(2, 2)); // 0.07 - 0.05 = 0.02
    }

    #[test]
    fn perfect_prediction_has_zero_divergence() {
        let input = AttributionInput {
            gross_spread: Decimal::new(5, 2),
            fees: Decimal::new(1, 2),
            slippage: Decimal::new(1, 2),
            funding: Decimal::ZERO,
        };
        let attr = compute_attribution(Ulid::new(), &input, &input).unwrap();
        assert_eq!(attr.divergence, Decimal::ZERO);
    }
}
