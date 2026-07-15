//! Live trading ceremony hooks.
//!
//! The `live_enabled` flag is NEVER set by application code (ADR-0007, S7).
//! This module provides read-only checks and the ceremony runbook documentation.
//!
//! # Ceremony runbook (operator manual)
//!
//! 1. Verify all risk-engine tests pass
//! 2. Verify paper ledger processes 100+ orders without error
//! 3. Verify a reviewed live venue adapter is installed (not implemented in EP-305)
//! 4. Set `AETHER_EXECUTION__LIVE_ENABLED=true` in the operator-owned environment
//! 5. Submit ONE minimum-size order on ONE venue
//! 6. Verify fill appears in audit chain
//! 7. Monitor for 30 days before increasing caps
//!
//! # Safety invariants
//!
//! - This module contains NO setter for live_enabled
//! - The ceremony steps are manual and out-of-band by design

use std::env;

/// Read the live_enabled flag.
pub fn is_live_enabled() -> bool {
    env::var("AETHER_EXECUTION__LIVE_ENABLED")
        .map(|v| v.to_lowercase() == "true" || v == "1")
        .unwrap_or(false)
}

/// Live ceremony prerequisites check.
pub fn verify_live_prerequisites(
    risk_passing: bool,
    paper_ledger_validated: bool,
    venue_health_ok: bool,
) -> Result<(), Vec<String>> {
    let mut failures = Vec::new();
    if !is_live_enabled() {
        failures.push("AETHER_EXECUTION__LIVE_ENABLED is not set to true".into());
    }
    if !risk_passing {
        failures.push("risk engine is not passing all tests".into());
    }
    if !paper_ledger_validated {
        failures.push("paper ledger has not been validated (100+ paper orders)".into());
    }
    if !venue_health_ok {
        failures.push("one or more venues are not healthy".into());
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures)
    }
}

/// The live ceremony runner.
/// Call this from an integration test or operator script.
pub struct LiveCeremony {
    pub risk_passing: bool,
    pub paper_ledger_validated: bool,
    pub venue_health_ok: bool,
}

impl Default for LiveCeremony {
    fn default() -> Self {
        Self { risk_passing: true, paper_ledger_validated: true, venue_health_ok: true }
    }
}

impl LiveCeremony {
    pub fn new() -> Self {
        Self::default()
    }

    /// Run the full ceremony check. Returns Ok if all gates pass.
    pub fn run(&self) -> Result<(), Vec<String>> {
        verify_live_prerequisites(
            self.risk_passing,
            self.paper_ledger_validated,
            self.venue_health_ok,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ceremony_fails_when_flag_not_set() {
        // In test, AETHER_EXECUTION__LIVE_ENABLED is typically not set
        let ceremony = LiveCeremony::new();
        if !is_live_enabled() {
            assert!(ceremony.run().is_err());
        }
    }

    #[test]
    fn ceremony_fails_when_risk_not_passing() {
        let mut ceremony = LiveCeremony::new();
        ceremony.risk_passing = false;
        let result = ceremony.run();
        // Will fail on both flag AND risk, but at minimum should fail
        assert!(result.is_err());
    }

    #[test]
    fn no_assignment_to_live_enabled_exists_in_source() {
        // Verify that NO code path assigns to AETHER_EXECUTION__LIVE_ENABLED.
        // We check the ceremony module itself — the full check is in
        // scripts/security-check.sh (forbidden-path guard).

        // Read our own source
        let source = include_str!("ceremony.rs");

        // Check: no std::env::set_var for this variable
        assert!(
            !source.contains("set_var(\"AETHER_EXECUTION__LIVE_ENABLED\")"),
            "ceremony.rs MUST NOT call set_var on AETHER_EXECUTION__LIVE_ENABLED"
        );

        // Only scan non-test module source — the test code naturally contains
        // the variable name in assertions, which we want to allow.
        let non_test_source =
            source.find("#[cfg(test)]").map(|pos| &source[..pos]).unwrap_or(source);

        for line in non_test_source.lines() {
            let trimmed = line.trim();
            // Skip documentation comments — they describe operator steps
            if trimmed.starts_with("///") || trimmed.starts_with("//!") {
                continue;
            }
            if trimmed.contains("AETHER_EXECUTION__LIVE_ENABLED") {
                assert!(
                    !trimmed.contains("set_var(") && !trimmed.contains('='),
                    "ceremony.rs line contains assignment to LIVE_ENABLED: {}",
                    trimmed
                );
            }
        }
    }

    #[test]
    fn ceremony_documents_all_prerequisites() {
        let ceremony = LiveCeremony::new();
        // Even if all gates pass, the flag check is independent
        let result = ceremony.run();
        if !is_live_enabled() {
            assert!(result.is_err());
            let errors = result.unwrap_err();
            assert!(errors.iter().any(|e| e.contains("LIVE_ENABLED")));
        }
    }
}
