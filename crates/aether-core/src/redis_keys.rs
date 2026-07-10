//! Redis key prefix constants. SPEC-002 authoritative key patterns.
//! Rule: any consumer must function (degraded latency acceptable) with Redis empty.

/// Redis key prefixes per SPEC-002 section "Redis".
/// All keys carry TTLs.
pub struct RedisKeys;

impl RedisKeys {
    /// Hot quote mirror: `q:quote:{market_key}` (ms TTL semantics via short expiry)
    pub const QUOTE_PREFIX: &str = "q:quote:";
    /// LLM response cache: `cache:llm:{prompt_hash}`
    pub const LLM_CACHE_PREFIX: &str = "cache:llm:";
    /// Semantic embedding cache: `cache:sem:{embedding_hash}` (Phase 3)
    pub const SEMANTIC_CACHE_PREFIX: &str = "cache:sem:";
    /// Session state: `sess:{session_id}`
    pub const SESSION_PREFIX: &str = "sess:";
    /// Rate-limit counters: `rl:{scope}`
    pub const RATE_LIMIT_PREFIX: &str = "rl:";
    /// Scheduler leases: `lease:{job}`
    pub const LEASE_PREFIX: &str = "lease:";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_prefix_matches_spec_002() {
        assert_eq!(RedisKeys::QUOTE_PREFIX, "q:quote:");
    }

    #[test]
    fn llm_cache_prefix_matches_spec_002() {
        assert_eq!(RedisKeys::LLM_CACHE_PREFIX, "cache:llm:");
    }

    #[test]
    fn semantic_cache_prefix_matches_spec_002() {
        assert_eq!(RedisKeys::SEMANTIC_CACHE_PREFIX, "cache:sem:");
    }

    #[test]
    fn session_prefix_matches_spec_002() {
        assert_eq!(RedisKeys::SESSION_PREFIX, "sess:");
    }

    #[test]
    fn rate_limit_prefix_matches_spec_002() {
        assert_eq!(RedisKeys::RATE_LIMIT_PREFIX, "rl:");
    }

    #[test]
    fn lease_prefix_matches_spec_002() {
        assert_eq!(RedisKeys::LEASE_PREFIX, "lease:");
    }

    #[test]
    fn all_prefixes_are_unique() {
        let prefixes = [
            RedisKeys::QUOTE_PREFIX,
            RedisKeys::LLM_CACHE_PREFIX,
            RedisKeys::SEMANTIC_CACHE_PREFIX,
            RedisKeys::SESSION_PREFIX,
            RedisKeys::RATE_LIMIT_PREFIX,
            RedisKeys::LEASE_PREFIX,
        ];
        let mut seen = std::collections::HashSet::new();
        for p in &prefixes {
            assert!(seen.insert(p), "Duplicate prefix: {}", p);
        }
    }
}
