//! Policy rule 4: USD-denominated limits.

use crate::proposal::PolicyStep;
use rust_decimal::Decimal;
use std::collections::HashMap;

/// Tracks spending limits.
#[derive(Debug, Clone)]
pub struct LimitTracker {
    pub per_tx_max_usd: Decimal,
    pub daily_max_usd: Decimal,
    pub per_destination_daily: HashMap<String, Decimal>,
    daily_spent: Decimal,
    per_destination_spent: HashMap<String, Decimal>,
}

impl LimitTracker {
    pub fn new(per_tx_usd: Decimal, daily_usd: Decimal) -> Self {
        Self {
            per_tx_max_usd: per_tx_usd,
            daily_max_usd: daily_usd,
            per_destination_daily: HashMap::new(),
            daily_spent: Decimal::ZERO,
            per_destination_spent: HashMap::new(),
        }
    }

    /// Check limits. Returns a PolicyStep indicating allow or deny.
    pub fn check_limits(&self, value_usd: Decimal, destination: &str) -> Option<PolicyStep> {
        // Per-tx check
        if value_usd > self.per_tx_max_usd {
            return Some(PolicyStep {
                rule: "limits".into(),
                result: "deny".into(),
                detail: format!(
                    "tx value ${} exceeds per-tx cap ${}",
                    value_usd, self.per_tx_max_usd
                ),
            });
        }
        // Daily check
        if self.daily_spent + value_usd > self.daily_max_usd {
            return Some(PolicyStep {
                rule: "limits".into(),
                result: "deny".into(),
                detail: format!(
                    "daily limit would be exceeded: {} + {} > {}",
                    self.daily_spent, value_usd, self.daily_max_usd
                ),
            });
        }
        // Per-destination check
        let dest_limit =
            self.per_destination_daily.get(destination).copied().unwrap_or(self.daily_max_usd);
        let dest_spent =
            self.per_destination_spent.get(destination).copied().unwrap_or(Decimal::ZERO);
        if dest_spent + value_usd > dest_limit {
            return Some(PolicyStep {
                rule: "limits".into(),
                result: "deny".into(),
                detail: format!("destination daily limit exceeded for {}", destination),
            });
        }
        Some(PolicyStep {
            rule: "limits".into(),
            result: "allow".into(),
            detail: format!(
                "within limits: ${} of daily ${}",
                self.daily_spent + value_usd,
                self.daily_max_usd
            ),
        })
    }

    /// Record a successful transaction against limits.
    pub fn record(&mut self, value_usd: Decimal, destination: &str) {
        self.daily_spent += value_usd;
        *self.per_destination_spent.entry(destination.to_string()).or_default() += value_usd;
    }

    /// Seed durable rolling usage before evaluating a new proposal.
    pub fn seed_usage(
        &mut self,
        daily_spent: Decimal,
        destination: &str,
        destination_spent: Decimal,
    ) {
        self.daily_spent = daily_spent;
        self.per_destination_spent.insert(destination.to_string(), destination_spent);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tx_within_limits_allowed() {
        let lt = LimitTracker::new(Decimal::new(1000, 2), Decimal::new(10000, 2)); // $10 per-tx, $100 daily
        let step = lt.check_limits(Decimal::new(500, 2), "0xdest").unwrap();
        assert_eq!(step.result, "allow");
    }

    #[test]
    fn tx_exceeding_per_tx_denied() {
        let lt = LimitTracker::new(Decimal::new(100, 2), Decimal::new(10000, 2)); // $1 per-tx
        let step = lt.check_limits(Decimal::new(500, 2), "0xdest").unwrap();
        assert_eq!(step.result, "deny");
    }

    #[test]
    fn cumulative_daily_limit_denied() {
        let mut lt = LimitTracker::new(Decimal::new(1000, 2), Decimal::new(500, 2)); // $10 per-tx, $5 daily
        lt.record(Decimal::new(400, 2), "0xdest"); // $4 spent
        let step = lt.check_limits(Decimal::new(200, 2), "0xdest").unwrap(); // $2 more = $6 > $5
        assert_eq!(step.result, "deny");
    }
}
