//! Opportunity scoring with deterministic confidence.
//! No LLM in the scan loop (INV-1).

use aether_core::opportunity::BrainRef;
use rust_decimal::Decimal;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvidenceSignal {
    /// Deterministic recall relevance, constrained by the caller to 0..=1.
    pub relevance: Decimal,
    /// Source trust/reliability weight, constrained by the caller to 0..=1.
    pub trust: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceSnapshot {
    pub signal: EvidenceSignal,
    pub explain_ref: BrainRef,
}

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
        Self { min_confidence: Decimal::new(5, 1) }
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
        evidence: Option<EvidenceSignal>,
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

        // Recall is computed outside the hot loop. The scanner consumes only
        // its deterministic relevance/trust snapshot; no LLM is invoked here.
        if let Some(evidence) = evidence {
            let evidence_weight = evidence.relevance.clamp(Decimal::ZERO, Decimal::ONE)
                * evidence.trust.clamp(Decimal::ZERO, Decimal::ONE);
            confidence *= Decimal::new(75, 2) + evidence_weight * Decimal::new(25, 2);
            features.push("recall_evidence_present".into());
        } else {
            confidence *= Decimal::new(75, 2);
            features.push("recall_evidence_missing".into());
        }

        let confidence = confidence.clamp(Decimal::ZERO, Decimal::ONE);
        let score = gross_spread.max(Decimal::ZERO) * confidence;
        ScoredOpportunity { confidence, score, features }
    }
}

impl Default for Scorer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scoring_is_deterministic() {
        let scorer = Scorer::new();
        let evidence = Some(EvidenceSignal { relevance: Decimal::ONE, trust: Decimal::ONE });
        let r1 = scorer.score(Decimal::new(5, 2), 100, 5000, 3, evidence);
        let r2 = scorer.score(Decimal::new(5, 2), 100, 5000, 3, evidence);
        assert_eq!(r1.confidence, r2.confidence);
        assert_eq!(r1.score, r2.score);
    }

    #[test]
    fn single_venue_zero_confidence() {
        let scorer = Scorer::new();
        let r = scorer.score(Decimal::new(10, 2), 100, 5000, 1, None);
        assert_eq!(r.confidence, Decimal::ZERO);
    }

    #[test]
    fn score_uses_bounded_confidence() {
        let scorer = Scorer::new();
        let result = scorer.score(
            Decimal::new(5, 2),
            0,
            5000,
            4,
            Some(EvidenceSignal { relevance: Decimal::ONE, trust: Decimal::ONE }),
        );
        assert_eq!(result.confidence, Decimal::ONE);
        assert_eq!(result.score, Decimal::new(5, 2));
    }

    #[test]
    fn missing_recall_evidence_degrades_confidence_visibly() {
        let scorer = Scorer::new();
        let result = scorer.score(Decimal::new(5, 2), 0, 5000, 2, None);
        assert_eq!(result.confidence, Decimal::new(75, 2));
        assert!(result.features.contains(&"recall_evidence_missing".to_owned()));
    }
}
