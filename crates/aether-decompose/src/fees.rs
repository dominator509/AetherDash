//! Venue-adjacent taker-fee schedules used by scanner and simulator.
//!
//! These are conservative fallbacks for deterministic paper estimates. A
//! market/account-specific rate supplied by a venue should replace the
//! fallback before live routing; execution remains outside this crate.

use rust_decimal::Decimal;
use serde::Deserialize;
use std::str::FromStr;
use thiserror::Error;

const EMBEDDED: [(&str, &str); 5] = [
    ("kalshi", include_str!("../../../connectors/venues/kalshi/fees.toml")),
    ("polymarket", include_str!("../../../connectors/venues/polymarket/fees.toml")),
    ("hyperliquid", include_str!("../../../connectors/venues/hyperliquid/fees.toml")),
    ("alpaca", include_str!("../../../connectors/venues/alpaca/fees.toml")),
    ("openbb", include_str!("../../../connectors/venues/openbb/fees.toml")),
];

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum FeeModel {
    ProbabilityCurve,
    NotionalRate,
    ReadOnly,
}

#[derive(Debug, Clone, Deserialize)]
struct RawSchedule {
    model: FeeModel,
    taker_rate: String,
}

#[derive(Debug, Clone)]
struct FeeSchedule {
    model: FeeModel,
    taker_rate: Decimal,
}

#[derive(Debug, Clone)]
pub struct FeeCatalog {
    schedules: std::collections::HashMap<String, FeeSchedule>,
}

#[derive(Debug, Error)]
pub enum FeeError {
    #[error("missing fee schedule for executable venue {0}")]
    MissingVenue(String),
    #[error("venue {0} is read-only and cannot form an executable opportunity")]
    ReadOnlyVenue(String),
    #[error("invalid fee schedule for {venue}: {detail}")]
    InvalidConfig { venue: String, detail: String },
}

impl FeeCatalog {
    pub fn load_embedded() -> Result<Self, FeeError> {
        let mut schedules = std::collections::HashMap::new();
        for (venue, source) in EMBEDDED {
            let raw: RawSchedule = toml::from_str(source).map_err(|error| {
                FeeError::InvalidConfig { venue: venue.to_owned(), detail: error.to_string() }
            })?;
            let taker_rate = Decimal::from_str(&raw.taker_rate).map_err(|error| {
                FeeError::InvalidConfig { venue: venue.to_owned(), detail: error.to_string() }
            })?;
            if !(Decimal::ZERO..=Decimal::ONE).contains(&taker_rate) {
                return Err(FeeError::InvalidConfig {
                    venue: venue.to_owned(),
                    detail: "taker_rate must be between zero and one".into(),
                });
            }
            schedules.insert(venue.to_owned(), FeeSchedule { model: raw.model, taker_rate });
        }
        Ok(Self { schedules })
    }

    /// Estimate both taker legs in the same currency units as gross P&L.
    pub fn estimate_pair(
        &self,
        buy_venue: &str,
        sell_venue: &str,
        buy_price: Decimal,
        sell_price: Decimal,
        size: Decimal,
    ) -> Result<Decimal, FeeError> {
        Ok(self.estimate_leg(buy_venue, buy_price, size)?
            + self.estimate_leg(sell_venue, sell_price, size)?)
    }

    pub fn is_executable(&self, venue: &str) -> Result<bool, FeeError> {
        let schedule =
            self.schedules.get(venue).ok_or_else(|| FeeError::MissingVenue(venue.to_owned()))?;
        Ok(schedule.model != FeeModel::ReadOnly)
    }

    fn estimate_leg(
        &self,
        venue: &str,
        price: Decimal,
        size: Decimal,
    ) -> Result<Decimal, FeeError> {
        let schedule =
            self.schedules.get(venue).ok_or_else(|| FeeError::MissingVenue(venue.to_owned()))?;
        let size = size.max(Decimal::ZERO);
        let price = price.max(Decimal::ZERO);
        match schedule.model {
            FeeModel::ProbabilityCurve => {
                let bounded = price.clamp(Decimal::ZERO, Decimal::ONE);
                Ok(size * schedule.taker_rate * bounded * (Decimal::ONE - bounded))
            }
            FeeModel::NotionalRate => Ok(size * price * schedule.taker_rate),
            FeeModel::ReadOnly => Err(FeeError::ReadOnlyVenue(venue.to_owned())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_schedules_parse_and_probability_fees_are_per_leg() {
        let catalog = FeeCatalog::load_embedded().unwrap();
        let fee = catalog
            .estimate_pair(
                "kalshi",
                "polymarket",
                Decimal::new(5, 1),
                Decimal::new(5, 1),
                Decimal::new(100, 0),
            )
            .unwrap();
        assert_eq!(fee, Decimal::new(35, 1));
    }

    #[test]
    fn read_only_venue_fails_closed() {
        let catalog = FeeCatalog::load_embedded().unwrap();
        assert!(matches!(
            catalog.estimate_pair("openbb", "kalshi", Decimal::ONE, Decimal::ONE, Decimal::ONE),
            Err(FeeError::ReadOnlyVenue(_))
        ));
    }
}
