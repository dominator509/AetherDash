//! Side-effect-free risk checks. Missing or malformed inputs fail closed.

use aether_core::market::{Market, MarketStatus};
use aether_core::order::{CapsSnapshot, OrderIntent, RiskReason, RiskReasonCode, Side};
use aether_core::quote::Quote;
use aether_core::time::UtcTime;
use rust_decimal::Decimal;

use crate::engine::{Balances, VenueHealthStatus};

fn reason(code: RiskReasonCode, detail: impl Into<String>) -> RiskReason {
    RiskReason { code, detail: detail.into() }
}

/// Returns the executable quote price for the intent side.
pub fn executable_price(quote: &Quote, side: Side) -> Option<Decimal> {
    match side {
        Side::Buy => quote.ask,
        Side::Sell => quote.bid,
        Side::BuyNo => quote.bid.map(|price| Decimal::ONE - price),
        Side::SellNo => quote.ask.map(|price| Decimal::ONE - price),
    }
    .filter(|price| *price > Decimal::ZERO)
}

pub fn check_liveness(
    intent: &OrderIntent,
    market: Option<&Market>,
    evaluated_at: UtcTime,
    max_staleness_ms: i64,
    max_future_skew_ms: i64,
) -> Option<RiskReason> {
    let Some(market) = market else {
        return Some(reason(RiskReasonCode::Liveness, "market metadata is unavailable"));
    };
    if market.key != intent.market || market.status != MarketStatus::Open {
        return Some(reason(RiskReasonCode::Liveness, "market is unavailable or not open"));
    }
    if intent.quote_snapshot.market != intent.market {
        return Some(reason(
            RiskReasonCode::Liveness,
            "quote snapshot market does not match intent",
        ));
    }
    let age = evaluated_at.unix_millis() - intent.quote_snapshot.ts.unix_millis();
    if age > max_staleness_ms {
        return Some(reason(RiskReasonCode::Liveness, "quote snapshot is stale"));
    }
    if age < -max_future_skew_ms {
        return Some(reason(RiskReasonCode::Liveness, "quote snapshot timestamp is in the future"));
    }
    if executable_price(&intent.quote_snapshot, intent.side).is_none() {
        return Some(reason(RiskReasonCode::Liveness, "quote snapshot has no executable price"));
    }
    None
}

/// Reject only adverse drift: a more favorable limit remains safe.
pub fn check_price_drift(intent: &OrderIntent, max_drift: Decimal) -> Option<RiskReason> {
    let limit = intent.limit_price?;
    let Some(reference) = executable_price(&intent.quote_snapshot, intent.side) else {
        return Some(reason(RiskReasonCode::PriceDrift, "price reference is unavailable"));
    };
    let adverse = match intent.side {
        Side::Buy | Side::BuyNo => limit - reference,
        Side::Sell | Side::SellNo => reference - limit,
    };
    if adverse > Decimal::ZERO && adverse / reference > max_drift {
        Some(reason(RiskReasonCode::PriceDrift, "limit price exceeds allowed adverse drift"))
    } else {
        None
    }
}

pub fn check_balance(
    intent: &OrderIntent,
    balances: Option<&Balances>,
    position: Option<Decimal>,
    notional: Decimal,
) -> Option<RiskReason> {
    match intent.side {
        Side::Buy | Side::BuyNo => match balances {
            Some(balance) if balance.free >= notional => None,
            Some(_) => Some(reason(RiskReasonCode::Balance, "insufficient free balance")),
            None => Some(reason(RiskReasonCode::Balance, "balance data is unavailable")),
        },
        Side::Sell | Side::SellNo => match position {
            Some(available) if available >= intent.size => None,
            Some(_) => Some(reason(RiskReasonCode::Balance, "insufficient position inventory")),
            None => Some(reason(RiskReasonCode::Balance, "position data is unavailable")),
        },
    }
}

pub fn check_venue_health(health: Option<&VenueHealthStatus>) -> Option<RiskReason> {
    match health {
        Some(value) if !value.breaker_open && value.status.eq_ignore_ascii_case("ok") => None,
        Some(_) => {
            Some(reason(RiskReasonCode::VenueHealth, "venue is unhealthy or its breaker is open"))
        }
        None => Some(reason(RiskReasonCode::VenueHealth, "venue health is unavailable")),
    }
}

pub fn check_caps(
    intent_caps: Option<&CapsSnapshot>,
    active_caps: &CapsSnapshot,
    balance_currency: Option<&str>,
    notional: Decimal,
    daily_notional: Decimal,
    hard_per_order_max: Decimal,
) -> Option<RiskReason> {
    let Some(intent_caps) = intent_caps else {
        return Some(reason(RiskReasonCode::CapExceeded, "intent caps snapshot is unavailable"));
    };
    let Some(currency) = balance_currency else {
        return Some(reason(RiskReasonCode::CapExceeded, "caps currency cannot be established"));
    };
    let currencies_match = [
        active_caps.per_order_max.currency.as_str(),
        active_caps.daily_max.currency.as_str(),
        intent_caps.per_order_max.currency.as_str(),
        intent_caps.daily_max.currency.as_str(),
    ]
    .iter()
    .all(|candidate| candidate.eq_ignore_ascii_case(currency));
    if !currencies_match {
        return Some(reason(RiskReasonCode::CapExceeded, "caps currency mismatch"));
    }

    let per_order = hard_per_order_max
        .min(active_caps.per_order_max.amount)
        .min(intent_caps.per_order_max.amount);
    let daily = active_caps.daily_max.amount.min(intent_caps.daily_max.amount);
    let Some(projected_daily) = daily_notional.checked_add(notional) else {
        return Some(reason(RiskReasonCode::CapExceeded, "daily notional overflow"));
    };
    if notional > per_order || projected_daily > daily {
        Some(reason(RiskReasonCode::CapExceeded, "order exceeds the lower effective cap"))
    } else {
        None
    }
}

pub fn check_jurisdiction(intent: &OrderIntent, eligible: Option<bool>) -> Option<RiskReason> {
    if intent.paper {
        return None;
    }
    match eligible {
        Some(true) => None,
        Some(false) => {
            Some(reason(RiskReasonCode::Jurisdiction, "venue is not eligible for live execution"))
        }
        None => {
            Some(reason(RiskReasonCode::Jurisdiction, "jurisdiction eligibility is unavailable"))
        }
    }
}
