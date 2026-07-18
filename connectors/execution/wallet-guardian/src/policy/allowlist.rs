//! Destination allowlist — restricts which addresses the Guardian can send to.

use std::collections::HashSet;

use crate::proposal::TxSpec;

/// Allowlist of approved destination addresses.
#[derive(Debug, Clone)]
pub struct AllowList {
    allowed: HashSet<String>,
    contract_calls: HashSet<(String, String)>,
}

impl AllowList {
    pub fn new() -> Self {
        Self { allowed: HashSet::new(), contract_calls: HashSet::new() }
    }

    pub fn with_allowed_contract_calls(mut self, calls: Vec<(&str, &str)>) -> Self {
        for (contract, selector) in calls {
            self.contract_calls.insert((contract.to_lowercase(), selector.to_lowercase()));
        }
        self
    }

    /// Add allowed destinations from an iterator.
    pub fn with_allowed_destinations(mut self, destinations: Vec<&str>) -> Self {
        for dest in destinations {
            self.allowed.insert(dest.to_lowercase());
        }
        self
    }

    /// Check if a destination is allowed.
    pub fn is_allowed(&self, destination: &str) -> bool {
        self.allowed.contains(&destination.to_lowercase())
    }

    /// Plain transfers use the destination allowlist. Contract interactions
    /// require the exact `(contract, four-byte selector)` pair.
    pub fn is_allowed_transaction(&self, tx: &TxSpec) -> bool {
        if tx.data.eq_ignore_ascii_case("0x") {
            return self.is_allowed(&tx.to);
        }
        if tx.data.len() < 10 || !tx.data.starts_with("0x") {
            return false;
        }
        self.contract_calls.contains(&(tx.to.to_lowercase(), tx.data[..10].to_lowercase()))
    }
}

impl Default for AllowList {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_list_denies_all() {
        let list = AllowList::new();
        assert!(!list.is_allowed("0x1234567890123456789012345678901234567890"));
    }

    #[test]
    fn enabled_list_rejects_unknown() {
        let list = AllowList::new()
            .with_allowed_destinations(vec!["0x1234567890123456789012345678901234567890"]);
        assert!(list.is_allowed("0x1234567890123456789012345678901234567890"));
        assert!(!list.is_allowed("0x0000000000000000000000000000000000000000"));
    }

    #[test]
    fn contract_call_requires_exact_selector_pair() {
        let list = AllowList::new().with_allowed_contract_calls(vec![(
            "0x1234567890123456789012345678901234567890",
            "0xa9059cbb",
        )]);
        let mut tx = TxSpec {
            chain_id: 1,
            to: "0x1234567890123456789012345678901234567890".into(),
            value: "0x0".into(),
            data: "0xa9059cbb0000".into(),
            gas_limit: 50_000,
            max_fee_per_gas: "0x1".into(),
            max_priority_fee_per_gas: "0x1".into(),
        };
        assert!(list.is_allowed_transaction(&tx));
        tx.data = "0x095ea7b30000".into();
        assert!(!list.is_allowed_transaction(&tx));
    }
}
