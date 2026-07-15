use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// How aggressively the fill model walks the order book.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Aggressiveness {
    /// Fill only at the best bid/ask level (no walking into the book).
    PassiveAtTouch,
    /// Walk into the book to fill the full size.
    CrossToDepth,
}

/// Configuration for the fill model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FillConfig {
    /// How aggressively to walk the book.
    pub aggressiveness: Aggressiveness,
    /// Multiplier applied to worst visible level price when extrapolating
    /// beyond visible depth.  Default 1.05 (5% extra cost for invisible liquidity).
    pub depth_exhaustion_multiplier: Decimal,
    /// Fraction of notional charged as a fee (10 bps = 0.001).
    pub fee_rate: Decimal,
    /// Currency tag used for generated fee amounts.
    pub fee_currency: String,
}

impl Default for FillConfig {
    fn default() -> Self {
        Self {
            aggressiveness: Aggressiveness::CrossToDepth,
            depth_exhaustion_multiplier: Decimal::new(105, 2), // 1.05
            fee_rate: Decimal::new(1, 3),                      // 0.001
            fee_currency: "USDC".to_owned(),
        }
    }
}

impl FillConfig {
    /// Passive-at-touch config (fills only at best bid/ask).
    pub fn passive() -> Self {
        Self { aggressiveness: Aggressiveness::PassiveAtTouch, ..Default::default() }
    }

    /// Validate configuration before it is used for execution math.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.depth_exhaustion_multiplier < Decimal::ONE {
            return Err("depth exhaustion multiplier must be at least 1");
        }
        if self.fee_rate < Decimal::ZERO {
            return Err("fee rate cannot be negative");
        }
        if self.fee_currency.trim().is_empty() {
            return Err("fee currency cannot be empty");
        }
        Ok(())
    }
}
