//! Opportunity scoring with deterministic confidence.
//! No LLM in the scan loop (INV-1).

use rust_decimal::Decimal;

#[derive(Debug, Clone)]
pub struct ScoredOpportunity {
    pub confidence: Decimal,
    pub score: Decimal,
    pub features: Vec<String>,
}

pub struct Scorer {
    pub min_confidence: Decimal,
}

impl Scorer {
    pub fn new() -> Self {
        Self {
            min_confidence: Decimal::new(5, 1),
        }
    }

    /// Compute confidence from deterministic features.
    /// Higher confidence when: spread is large, venues are established,
    /// quotes are fresh, volumes are high.
    pub fn score(
        &self,
        gross_spread: Decimal,
        quote_age_ms: i64,
        max_age_ms: i64,
        venue_count: usize,
    ) -> ScoredOpportunity {
        let mut confidence = Decimal::ONE;
        let mut features = Vec::new();

        // Spread factor: wider spread = higher confidence
        if gross_spread < Decimal::new(2, 2) {
            // < 2%
            confidence *= Decimal::new(8, 1); // 0.8x
            features.push("spread_narrow".into());
        } else {
            features.push("spread_wide".into());
        }

        // Freshness factor
        if quote_age_ms > max_age_ms / 2 {
            confidence *= Decimal::new(7, 1); // 0.7x for somewhat stale
            features.push("quote_moderately_stale".into());
        } else {
            features.push("quote_fresh".into());
        }

        // Venue diversity factor
        if venue_count < 2 {
            confidence = Decimal::ZERO;
            features.push("insufficient_venues".into());
        } else if venue_count >= 4 {
            confidence *= Decimal::new(12, 1); // 1.2x bonus for many venues
            features.push("venue_diverse".into());
        }

        let score = gross_spread * confidence;
        ScoredOpportunity {
            confidence: confidence.min(Decimal::ONE),
            score,
            features,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scoring_is_deterministic() {
        let scorer = Scorer::new();
        let r1 = scorer.score(Decimal::new(5, 2), 100, 5000, 3);
        let r2 = scorer.score(Decimal::new(5, 2), 100, 5000, 3);
        assert_eq!(r1.confidence, r2.confidence);
        assert_eq!(r1.score, r2.score);
    }

    #[test]
    fn single_venue_zero_confidence() {
        let scorer = Scorer::new();
        let r = scorer.score(Decimal::new(10, 2), 100, 5000, 1);
        assert_eq!(r.confidence, Decimal::ZERO);
    }
}
