//! Policy engine — chains allowlist -> simulation -> limits -> routing -> gas checks.
//!
//! The engine evaluates every transaction proposal against a configurable
//! policy chain. A single "deny" step rejects the proposal.

use crate::policy::allowlist::AllowList;
use crate::policy::limits::LimitTracker;
use crate::policy::simulation::{simulate, SimulationResult};
use crate::proposal::{PolicyStep, TxSpec};
use rust_decimal::Decimal;

/// Configuration for the policy engine.
#[derive(Debug, Clone)]
pub struct PolicyConfig {
    /// Maximum simulated USD delta before routing to human approval.
    pub max_auto_approve_usd: Decimal,
    /// Maximum gas price (in gwei) before requiring step-up.
    pub max_gas_price_gwei: u64,
    /// Chain IDs where transactions are allowed.
    pub allowed_chains: Vec<u64>,
    pub per_tx_max_usd: Decimal,
    pub daily_max_usd: Decimal,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            max_auto_approve_usd: Decimal::new(1, 1), // $0.10 fail-safe default
            max_gas_price_gwei: 100,
            allowed_chains: vec![1, 137, 42161], // mainnet, polygon, arbitrum
            per_tx_max_usd: Decimal::new(100_000, 2),
            daily_max_usd: Decimal::new(500_000, 2),
        }
    }
}

/// Result of a policy evaluation.
#[derive(Debug, Clone)]
pub struct PolicyResult {
    pub allowed: bool,
    pub requires_human: bool,
    pub trace: Vec<PolicyStep>,
}

/// Policy engine with configurable rule chain.
pub struct PolicyEngine {
    pub allowlist: AllowList,
    pub config: PolicyConfig,
    pub limits: LimitTracker,
}

impl PolicyEngine {
    pub fn new(config: PolicyConfig) -> Self {
        let limits = LimitTracker::new(config.per_tx_max_usd, config.daily_max_usd);
        Self { allowlist: AllowList::new(), config, limits }
    }

    /// Evaluate a transaction against the full policy chain.
    ///
    /// Policy chain order:
    /// 1. chain allowlist
    /// 2. destination allowlist
    /// 3. deterministic local simulation gate
    /// 4. USD limits
    /// 5. approval routing
    /// 6. gas fee cap
    pub fn evaluate(
        &self,
        tx: &TxSpec,
        value_usd: Decimal,
        is_withdrawal: bool,
        actor_tier: u8,
    ) -> PolicyResult {
        let simulation = simulate(tx, tx.chain_id, value_usd);
        self.evaluate_with_simulation(tx, simulation, is_withdrawal, actor_tier)
    }

    /// Evaluate a transaction with a precomputed simulation result.
    ///
    /// This is the production seam for RPC/anvil simulation: the policy engine
    /// consumes what the transaction DOES (`value_delta_usd`), not just a caller
    /// supplied nominal value.
    pub fn evaluate_with_simulation(
        &self,
        tx: &TxSpec,
        simulation: SimulationResult,
        is_withdrawal: bool,
        actor_tier: u8,
    ) -> PolicyResult {
        let mut trace: Vec<PolicyStep> = Vec::new();

        // Step 0: chain validation
        if !self.config.allowed_chains.contains(&tx.chain_id) {
            trace.push(PolicyStep {
                rule: "chain".into(),
                result: "deny".into(),
                detail: format!("chain {} not in allowed chains", tx.chain_id),
            });
            return PolicyResult { allowed: false, requires_human: false, trace };
        }
        trace.push(PolicyStep {
            rule: "chain".into(),
            result: "allow".into(),
            detail: format!("chain {} allowed", tx.chain_id),
        });

        // Step 1: allowlist
        if !self.allowlist.is_allowed_transaction(tx) {
            trace.push(PolicyStep {
                rule: "allowlist".into(),
                result: "deny".into(),
                detail: format!("destination {} not in allowlist", tx.to),
            });
            return PolicyResult { allowed: false, requires_human: false, trace };
        }
        trace.push(PolicyStep {
            rule: "allowlist".into(),
            result: "allow".into(),
            detail: "destination allowed".into(),
        });

        // Step 2: simulation
        if !simulation.success {
            trace.push(PolicyStep {
                rule: "simulation".into(),
                result: "deny".into(),
                detail: simulation.error.unwrap_or_else(|| "simulation_failed".into()),
            });
            return PolicyResult { allowed: false, requires_human: false, trace };
        }
        trace.push(PolicyStep {
            rule: "simulation".into(),
            result: "allow".into(),
            detail: format!(
                "simulation passed; gas_used={}; value_delta_usd={}",
                simulation.gas_used.unwrap_or(0),
                simulation.value_delta_usd
            ),
        });

        if tx_has_value(tx) && simulation.value_delta_usd <= Decimal::ZERO {
            trace.push(PolicyStep {
                rule: "limits".into(),
                result: "deny".into(),
                detail: "stale or unavailable price for non-zero value transfer".into(),
            });
            return PolicyResult { allowed: false, requires_human: false, trace };
        }

        // Step 3: value limits
        let limit_step = self
            .limits
            .check_limits(simulation.value_delta_usd, &tx.to)
            .unwrap_or_else(|| PolicyStep {
                rule: "limits".into(),
                result: "deny".into(),
                detail: "limits unavailable".into(),
            });
        if limit_step.result == "deny" {
            trace.push(limit_step);
            return PolicyResult { allowed: false, requires_human: false, trace };
        }
        trace.push(limit_step);

        // Step 4: routing (withdrawal always human)
        let requires_human = is_withdrawal
            || simulation.value_delta_usd > self.config.max_auto_approve_usd
            || actor_tier < 4;
        if requires_human {
            trace.push(PolicyStep {
                rule: "approval_routing".into(),
                result: "pending_human".into(),
                detail: if is_withdrawal {
                    "withdrawals always require human approval".into()
                } else if actor_tier < 4 {
                    "auto approval requires tier 4 or 5".into()
                } else {
                    format!(
                        "value {} exceeds auto-approve threshold {}",
                        simulation.value_delta_usd, self.config.max_auto_approve_usd
                    )
                },
            });
        } else {
            trace.push(PolicyStep {
                rule: "approval_routing".into(),
                result: "auto_approved".into(),
                detail: "within auto-approval limits".into(),
            });
        }

        // Step 5: gas sanity check
        let Some(max_fee) = parse_hex_quantity(&tx.max_fee_per_gas) else {
            trace.push(PolicyStep {
                rule: "gas".into(),
                result: "deny".into(),
                detail: "max fee is not a canonical hexadecimal quantity".into(),
            });
            return PolicyResult { allowed: false, requires_human: false, trace };
        };
        let Some(priority_fee) = parse_hex_quantity(&tx.max_priority_fee_per_gas) else {
            trace.push(PolicyStep {
                rule: "gas".into(),
                result: "deny".into(),
                detail: "priority fee is not a canonical hexadecimal quantity".into(),
            });
            return PolicyResult { allowed: false, requires_human: false, trace };
        };
        if priority_fee > max_fee {
            trace.push(PolicyStep {
                rule: "gas".into(),
                result: "deny".into(),
                detail: "priority fee exceeds max fee".into(),
            });
            return PolicyResult { allowed: false, requires_human: false, trace };
        }
        let max_fee_gwei = max_fee / 1_000_000_000;
        if max_fee_gwei > self.config.max_gas_price_gwei as u128 {
            trace.push(PolicyStep {
                rule: "gas".into(),
                result: "deny".into(),
                detail: format!(
                    "max fee {} gwei exceeds limit {} gwei",
                    max_fee_gwei, self.config.max_gas_price_gwei
                ),
            });
            return PolicyResult { allowed: false, requires_human: false, trace };
        }
        trace.push(PolicyStep {
            rule: "gas".into(),
            result: "allow".into(),
            detail: "gas price within limits".into(),
        });

        PolicyResult { allowed: true, requires_human, trace }
    }
}

fn tx_has_value(tx: &TxSpec) -> bool {
    parse_hex_quantity(&tx.value).is_some_and(|value| value > 0)
}

fn parse_hex_quantity(value: &str) -> Option<u128> {
    let digits = value.strip_prefix("0x")?;
    if digits.is_empty()
        || (digits.len() > 1 && digits.starts_with('0'))
        || !digits.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return None;
    }
    u128::from_str_radix(digits, 16).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tx() -> TxSpec {
        TxSpec {
            chain_id: 137,
            to: "0x1234567890123456789012345678901234567890".into(),
            value: "0x0".into(),
            data: "0x".into(),
            gas_limit: 100000,
            max_fee_per_gas: "0x3b9aca00".into(),
            max_priority_fee_per_gas: "0x3b9aca00".into(),
        }
    }

    #[test]
    fn unknown_destination_denied_when_allowlist_enabled() {
        let engine = PolicyEngine {
            allowlist: AllowList::new()
                .with_allowed_destinations(vec!["0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"]),
            ..PolicyEngine::new(PolicyConfig::default())
        };
        let tx = make_tx(); // to: 0x12345... is not in the allowlist
        let result = engine.evaluate(&tx, Decimal::ZERO, false, 5);
        assert!(!result.allowed);
        assert!(result.trace.iter().any(|s| s.rule == "allowlist" && s.result == "deny"));
    }

    #[test]
    fn withdrawal_always_routes_to_human() {
        let engine = PolicyEngine {
            allowlist: AllowList::new()
                .with_allowed_destinations(vec!["0x1234567890123456789012345678901234567890"]),
            ..PolicyEngine::new(PolicyConfig::default())
        };
        let result = engine.evaluate(&make_tx(), Decimal::new(1, 2), true, 5);
        let routing = result.trace.iter().find(|s| s.rule == "approval_routing").unwrap();
        assert_eq!(routing.result, "pending_human");
        assert!(result.requires_human);
    }

    #[test]
    fn stale_price_for_value_transfer_denied_at_limits() {
        let engine = PolicyEngine {
            allowlist: AllowList::new()
                .with_allowed_destinations(vec!["0x1234567890123456789012345678901234567890"]),
            ..PolicyEngine::new(PolicyConfig::default())
        };
        let mut tx = make_tx();
        tx.value = "0x1".into();
        let simulation = SimulationResult {
            success: true,
            gas_used: Some(21_000),
            error: None,
            value_delta_usd: Decimal::ZERO,
        };
        let result = engine.evaluate_with_simulation(&tx, simulation, false, 5);
        assert!(!result.allowed);
        assert!(result.trace.iter().any(|s| s.rule == "limits" && s.detail.contains("stale")));
    }

    #[test]
    fn limits_use_simulated_balance_delta_not_nominal_input() {
        let engine = PolicyEngine {
            allowlist: AllowList::new()
                .with_allowed_destinations(vec!["0x1234567890123456789012345678901234567890"]),
            ..PolicyEngine::new(PolicyConfig {
                per_tx_max_usd: Decimal::new(100, 0),
                daily_max_usd: Decimal::new(1_000, 0),
                max_auto_approve_usd: Decimal::new(1_000, 0),
                ..PolicyConfig::default()
            })
        };
        let simulation = SimulationResult {
            success: true,
            gas_used: Some(21_000),
            error: None,
            value_delta_usd: Decimal::new(101, 0),
        };
        let result = engine.evaluate_with_simulation(&make_tx(), simulation, false, 5);
        assert!(!result.allowed);
        assert!(result.trace.iter().any(|s| s.rule == "limits" && s.result == "deny"));
    }

    #[test]
    fn human_routing_does_not_bypass_gas_cap() {
        let engine = PolicyEngine {
            allowlist: AllowList::new()
                .with_allowed_destinations(vec!["0x1234567890123456789012345678901234567890"]),
            ..PolicyEngine::new(PolicyConfig::default())
        };
        let mut tx = make_tx();
        tx.value = "0x1".into();
        tx.max_fee_per_gas = "0x178411b200".into(); // 101 gwei
        let result = engine.evaluate(&tx, Decimal::new(1, 2), true, 5);
        assert!(!result.allowed);
        assert!(result.trace.iter().any(|step| step.rule == "gas" && step.result == "deny"));
    }

    #[test]
    fn malformed_fee_fails_closed_instead_of_becoming_zero() {
        let engine = PolicyEngine {
            allowlist: AllowList::new()
                .with_allowed_destinations(vec!["0x1234567890123456789012345678901234567890"]),
            ..PolicyEngine::new(PolicyConfig::default())
        };
        let mut tx = make_tx();
        tx.max_fee_per_gas = "not-hex".into();
        let result = engine.evaluate(&tx, Decimal::ZERO, false, 5);
        assert!(!result.allowed);
        assert!(result.trace.iter().any(|step| {
            step.rule == "gas" && step.result == "deny" && step.detail.contains("not a canonical")
        }));
    }

    #[test]
    fn priority_fee_above_max_fee_is_denied() {
        let engine = PolicyEngine {
            allowlist: AllowList::new()
                .with_allowed_destinations(vec!["0x1234567890123456789012345678901234567890"]),
            ..PolicyEngine::new(PolicyConfig::default())
        };
        let mut tx = make_tx();
        tx.max_fee_per_gas = "0x1".into();
        tx.max_priority_fee_per_gas = "0x2".into();
        let result = engine.evaluate(&tx, Decimal::ZERO, false, 5);
        assert!(!result.allowed);
        assert!(result.trace.iter().any(|step| {
            step.rule == "gas" && step.result == "deny" && step.detail.contains("exceeds")
        }));
    }
}
