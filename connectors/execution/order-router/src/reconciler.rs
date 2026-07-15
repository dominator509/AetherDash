//! Pure state tracker for orders whose venue submission outcome is unknown.
//!
//! This module never retries a submission. A future live adapter may feed
//! authoritative venue observations into it after a timeout.

use aether_core::ids::{Ulid, VenueId};
use std::collections::HashMap;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VenueObservation {
    Filled,
    Open,
    Cancelled,
    Rejected,
    NotFound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconciledStatus {
    Unknown,
    Filled,
    Open,
    Closed,
    NotFound,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconciliationTarget {
    pub order_id: Ulid,
    pub venue_id: VenueId,
    pub client_order_id: String,
    pub attempts: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReconciliationEntry {
    status: ReconciledStatus,
    venue_id: VenueId,
    client_order_id: String,
    attempts: u32,
}

impl ReconciledStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Filled | Self::Closed | Self::NotFound)
    }
}

/// Configuration for the reconciler.
#[derive(Debug, Clone)]
pub struct ReconcilerConfig {
    /// How long to wait before declaring a submit as timed out.
    pub submit_timeout: Duration,
    /// Maximum number of reconciliation attempts before giving up.
    pub max_attempts: u32,
}

impl Default for ReconcilerConfig {
    fn default() -> Self {
        Self { submit_timeout: Duration::from_secs(30), max_attempts: 3 }
    }
}

#[derive(Debug, Default)]
pub struct Reconciler {
    orders: HashMap<Ulid, ReconciliationEntry>,
}

impl Reconciler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a timed-out submission exactly once. The caller must reconcile
    /// this ID before considering any new business action.
    pub fn begin_unknown(
        &mut self,
        order_id: Ulid,
        venue_id: VenueId,
        client_order_id: String,
    ) -> bool {
        if self.orders.contains_key(&order_id) {
            return false;
        }
        self.orders.insert(
            order_id,
            ReconciliationEntry {
                status: ReconciledStatus::Unknown,
                venue_id,
                client_order_id,
                attempts: 0,
            },
        );
        true
    }

    /// Apply an authoritative observation. Terminal outcomes cannot regress.
    pub fn observe(&mut self, order_id: Ulid, observation: VenueObservation) -> bool {
        let Some(current) = self.orders.get_mut(&order_id) else {
            return false;
        };
        if current.status.is_terminal() {
            return false;
        }
        current.status = match observation {
            VenueObservation::Filled => ReconciledStatus::Filled,
            VenueObservation::Open => ReconciledStatus::Open,
            VenueObservation::Cancelled | VenueObservation::Rejected => ReconciledStatus::Closed,
            VenueObservation::NotFound => ReconciledStatus::NotFound,
        };
        true
    }

    pub fn status(&self, order_id: &Ulid) -> Option<ReconciledStatus> {
        self.orders.get(order_id).map(|entry| entry.status)
    }

    pub fn requires_reconciliation(&self, order_id: &Ulid) -> bool {
        self.status(order_id).is_some_and(|status| !status.is_terminal())
    }

    /// Number of orders whose status is not yet terminal.
    pub fn pending_count(&self) -> usize {
        self.orders.iter().filter(|(_, entry)| !entry.status.is_terminal()).count()
    }

    /// All order IDs still in [`ReconciledStatus::Unknown`].
    pub fn pending_ids(&self) -> Vec<Ulid> {
        self.orders
            .iter()
            .filter(|(_, entry)| entry.status == ReconciledStatus::Unknown)
            .map(|(id, _)| *id)
            .collect()
    }

    pub fn pending_targets(&self) -> Vec<ReconciliationTarget> {
        self.orders
            .iter()
            .filter(|(_, entry)| !entry.status.is_terminal())
            .map(|(order_id, entry)| ReconciliationTarget {
                order_id: *order_id,
                venue_id: entry.venue_id.clone(),
                client_order_id: entry.client_order_id.clone(),
                attempts: entry.attempts,
            })
            .collect()
    }

    pub fn record_attempt(&mut self, order_id: &Ulid) -> Option<u32> {
        let entry = self.orders.get_mut(order_id)?;
        if entry.status.is_terminal() {
            return None;
        }
        entry.attempts = entry.attempts.saturating_add(1);
        Some(entry.attempts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_order_is_registered_once_and_never_retried_implicitly() {
        let id = Ulid::new();
        let venue = VenueId::new("alpaca").unwrap();
        let mut reconciler = Reconciler::new();
        assert!(reconciler.begin_unknown(id, venue.clone(), id.to_string()));
        assert!(!reconciler.begin_unknown(id, venue, id.to_string()));
        assert_eq!(reconciler.status(&id), Some(ReconciledStatus::Unknown));
    }

    #[test]
    fn pending_count_tracks_non_terminal_orders() {
        let mut reconciler = Reconciler::new();
        assert_eq!(reconciler.pending_count(), 0);
        let id1 = Ulid::new();
        let id2 = Ulid::new();
        let venue = VenueId::new("alpaca").unwrap();
        reconciler.begin_unknown(id1, venue.clone(), id1.to_string());
        reconciler.begin_unknown(id2, venue, id2.to_string());
        assert_eq!(reconciler.pending_count(), 2);
        reconciler.observe(id1, VenueObservation::Filled);
        assert_eq!(reconciler.pending_count(), 1);
    }

    #[test]
    fn pending_ids_returns_unknown_orders() {
        let mut reconciler = Reconciler::new();
        let id = Ulid::new();
        reconciler.begin_unknown(id, VenueId::new("alpaca").unwrap(), id.to_string());
        assert_eq!(reconciler.pending_ids(), vec![id]);
    }

    #[test]
    fn pending_targets_include_query_reference_and_attempt_count() {
        let mut reconciler = Reconciler::new();
        let id = Ulid::new();
        let venue = VenueId::new("alpaca").unwrap();
        reconciler.begin_unknown(id, venue.clone(), "client-1".into());
        assert_eq!(reconciler.record_attempt(&id), Some(1));

        let targets = reconciler.pending_targets();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].order_id, id);
        assert_eq!(targets[0].venue_id, venue);
        assert_eq!(targets[0].client_order_id, "client-1");
        assert_eq!(targets[0].attempts, 1);
    }

    #[test]
    fn reconciler_config_defaults() {
        let config = ReconcilerConfig::default();
        assert_eq!(config.submit_timeout, Duration::from_secs(30));
        assert_eq!(config.max_attempts, 3);
    }

    #[test]
    fn terminal_observation_cannot_regress() {
        let id = Ulid::new();
        let mut reconciler = Reconciler::new();
        assert!(reconciler.begin_unknown(id, VenueId::new("alpaca").unwrap(), id.to_string()));
        assert!(reconciler.observe(id, VenueObservation::Filled));
        assert!(!reconciler.observe(id, VenueObservation::Open));
        assert_eq!(reconciler.status(&id), Some(ReconciledStatus::Filled));
    }
}
