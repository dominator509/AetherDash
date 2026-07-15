use aether_core::ids::{MarketKey, Ulid, VenueId};
use aether_core::market::Market;
use aether_core::order::{
    CapsSnapshot, OrderIntent, RiskReason, RiskReasonCode, RiskVerdict, RiskVerdictStatus,
};
use aether_core::time::UtcTime;
use rust_decimal::Decimal;
use std::collections::HashMap;

use crate::checks;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PositionOutcome {
    Yes,
    No,
}

#[derive(Debug, Clone)]
pub struct Balances {
    pub free: Decimal,
    pub locked: Decimal,
    pub currency: String,
}

#[derive(Debug, Clone)]
pub struct VenueHealthStatus {
    pub status: String,
    pub breaker_open: bool,
}

/// Complete, caller-snapshotted inputs for a deterministic evaluation.
#[derive(Debug, Clone)]
pub struct RiskContext {
    pub evaluated_at: UtcTime,
    pub markets: HashMap<MarketKey, Market>,
    pub balances: HashMap<VenueId, Balances>,
    /// Available inventory by market and binary outcome. Keeping YES and NO
    /// separate prevents a YES position from authorizing a NO sale.
    pub positions: HashMap<(MarketKey, PositionOutcome), Decimal>,
    pub venue_health: HashMap<VenueId, VenueHealthStatus>,
    pub active_caps: CapsSnapshot,
    pub caps_by_version: HashMap<Ulid, CapsSnapshot>,
    pub daily_notional: Decimal,
    /// Precomputed by the operator-owned jurisdiction policy. The risk engine
    /// consumes this decision and does not reinterpret venue flags.
    pub jurisdiction_eligible: HashMap<VenueId, bool>,
    pub live_enabled: bool,
}

#[derive(Debug, Clone)]
pub struct RiskConfig {
    pub max_drift: Decimal,
    pub max_quote_staleness_ms: i64,
    pub max_future_skew_ms: i64,
    pub hard_per_order_max: Decimal,
}

impl Default for RiskConfig {
    fn default() -> Self {
        Self {
            max_drift: Decimal::new(2, 2),
            max_quote_staleness_ms: 5_000,
            max_future_skew_ms: 1_000,
            hard_per_order_max: Decimal::new(10_000, 0),
        }
    }
}

pub struct RiskEngine {
    config: RiskConfig,
}

impl RiskEngine {
    pub fn new(config: RiskConfig) -> Self {
        Self { config }
    }
    pub fn with_defaults() -> Self {
        Self::new(RiskConfig::default())
    }
    pub fn config(&self) -> &RiskConfig {
        &self.config
    }

    /// Evaluate solely from the intent and supplied snapshot context.
    pub fn evaluate(&self, intent: &OrderIntent, context: &RiskContext) -> RiskVerdict {
        let mut reasons = Vec::new();
        if !intent.paper && !context.live_enabled {
            reasons.push(RiskReason {
                code: RiskReasonCode::LiveDisabled,
                detail: "live execution is disabled".into(),
            });
        }

        let market = context.markets.get(&intent.market);
        if let Some(reason) = checks::check_liveness(
            intent,
            market,
            context.evaluated_at,
            self.config.max_quote_staleness_ms,
            self.config.max_future_skew_ms,
        ) {
            reasons.push(reason);
        }
        if let Some(reason) = checks::check_price_drift(intent, self.config.max_drift) {
            reasons.push(reason);
        }

        let venue = market.map(|value| &value.venue);
        let executable_price = checks::executable_price(&intent.quote_snapshot, intent.side);
        // Use the greater of the executable quote and limit so caps/balance
        // never underestimate the cash value that an accepted order permits.
        let valuation_price = executable_price
            .map(|price| intent.limit_price.map_or(price, |limit| price.max(limit)));
        let notional = valuation_price
            .and_then(|price| price.checked_mul(intent.size))
            .filter(|value| *value >= Decimal::ZERO);
        match (venue, notional) {
            (Some(venue), Some(notional)) => {
                let balances = context.balances.get(venue);
                let position_outcome = match intent.side {
                    aether_core::order::Side::Sell => Some(PositionOutcome::Yes),
                    aether_core::order::Side::SellNo => Some(PositionOutcome::No),
                    _ => None,
                };
                let position = position_outcome.and_then(|outcome| {
                    context.positions.get(&(intent.market.clone(), outcome)).copied()
                });
                if let Some(reason) = checks::check_balance(intent, balances, position, notional) {
                    reasons.push(reason);
                }
                if let Some(reason) = checks::check_venue_health(context.venue_health.get(venue)) {
                    reasons.push(reason);
                }
                if let Some(reason) = checks::check_caps(
                    context.caps_by_version.get(&intent.caps_version),
                    &context.active_caps,
                    balances.map(|value| value.currency.as_str()),
                    notional,
                    context.daily_notional,
                    self.config.hard_per_order_max,
                ) {
                    reasons.push(reason);
                }
                if let Some(reason) = checks::check_jurisdiction(
                    intent,
                    context.jurisdiction_eligible.get(venue).copied(),
                ) {
                    reasons.push(reason);
                }
            }
            _ => {
                reasons.push(RiskReason {
                    code: RiskReasonCode::Liveness,
                    detail: "risk valuation inputs are unavailable".into(),
                });
            }
        }

        RiskVerdict {
            intent_id: intent.id,
            verdict: if reasons.is_empty() {
                RiskVerdictStatus::Allow
            } else {
                RiskVerdictStatus::Deny
            },
            reasons,
            ts: context.evaluated_at,
        }
    }
}
