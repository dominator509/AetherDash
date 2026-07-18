//! Deduplicate opportunities against open chains.
//! Same legs + same kind -> update the existing chain, don't create new one.

use aether_core::ids::MarketKey;
use aether_core::opportunity::OpportunityKind;
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct OpenChain {
    pub id: aether_core::ids::Ulid,
    pub kind: OpportunityKind,
    pub legs: HashSet<MarketKey>,
}

#[derive(Debug, Default)]
pub struct Deduplicator {
    open_chains: Vec<OpenChain>,
}

impl Deduplicator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, chain: OpenChain) {
        self.open_chains.push(chain);
    }

    /// Check if a detected opportunity is a duplicate of an open chain.
    /// Returns the chain ID if duplicate, None if new.
    pub fn check(
        &self,
        buy_leg: &MarketKey,
        sell_leg: &MarketKey,
        kind: OpportunityKind,
    ) -> Option<aether_core::ids::Ulid> {
        for chain in &self.open_chains {
            if chain.kind == kind
                && chain.legs.len() == 2
                && chain.legs.contains(buy_leg)
                && chain.legs.contains(sell_leg)
            {
                return Some(chain.id);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_core::ids::{Ulid, VenueId};

    #[test]
    fn deduplicates_same_legs() {
        let mut dedup = Deduplicator::new();
        let mk1 = MarketKey::new(&VenueId::new("kalshi").unwrap(), "a").unwrap();
        let mk2 = MarketKey::new(&VenueId::new("polymarket").unwrap(), "a").unwrap();
        let mk3 = MarketKey::new(&VenueId::new("kalshi").unwrap(), "b").unwrap();

        dedup.register(OpenChain {
            id: Ulid::new(),
            kind: OpportunityKind::Arbitrage,
            legs: [mk1.clone(), mk2.clone()].into(),
        });

        assert!(dedup.check(&mk1, &mk2, OpportunityKind::Arbitrage).is_some());
        assert!(dedup.check(&mk2, &mk1, OpportunityKind::Arbitrage).is_some());
        assert!(dedup.check(&mk1, &mk3, OpportunityKind::Arbitrage).is_none());
        assert!(dedup.check(&mk1, &mk2, OpportunityKind::Hedge).is_none());
    }
}
