//! Destination allowlist — restricts which addresses the Guardian can send to.

use std::collections::HashSet;

/// Allowlist of approved destination addresses.
#[derive(Debug, Clone)]
pub struct AllowList {
    allowed: HashSet<String>,
}

impl AllowList {
    pub fn new() -> Self {
        Self { allowed: HashSet::new() }
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
}
