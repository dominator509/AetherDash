//! Redis key prefix constants. SPEC-002 redis key patterns.
//! Centralized key namespace to prevent collisions and document usage.

/// Redis key prefixes for all AETHER services.
/// Pattern: `aether:<domain>:<entity>`
pub struct RedisKeys;

impl RedisKeys {
    /// Hot cache for market quotes: `aether:quote:{market_key}`
    pub const QUOTE_PREFIX: &str = "aether:quote:";
    /// Order book snapshot cache: `aether:book:{market_key}:{depth}`
    pub const BOOK_PREFIX: &str = "aether:book:";
    /// LLM prompt/semantic cache: `aether:llm:cache:{hash}`
    pub const LLM_CACHE_PREFIX: &str = "aether:llm:cache:";
    /// Risk engine caps cache: `aether:caps:{caps_version}`
    pub const CAPS_PREFIX: &str = "aether:caps:";
    /// Rate limiter state: `aether:rate:{key}`
    pub const RATE_LIMIT_PREFIX: &str = "aether:rate:";
    /// Session state: `aether:session:{session_id}`
    pub const SESSION_PREFIX: &str = "aether:session:";
    /// Circuit breaker state: `aether:breaker:{venue}:{kind}`
    pub const BREAKER_PREFIX: &str = "aether:breaker:";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redis_key_prefixes_are_unique() {
        let prefixes = [
            RedisKeys::QUOTE_PREFIX,
            RedisKeys::BOOK_PREFIX,
            RedisKeys::LLM_CACHE_PREFIX,
            RedisKeys::CAPS_PREFIX,
            RedisKeys::RATE_LIMIT_PREFIX,
            RedisKeys::SESSION_PREFIX,
            RedisKeys::BREAKER_PREFIX,
        ];
        // No duplicate prefixes
        let mut seen = std::collections::HashSet::new();
        for p in &prefixes {
            assert!(seen.insert(p), "Duplicate prefix: {}", p);
        }
    }

    #[test]
    fn redis_key_prefixes_follow_pattern() {
        for p in &[RedisKeys::QUOTE_PREFIX, RedisKeys::BOOK_PREFIX, RedisKeys::LLM_CACHE_PREFIX] {
            assert!(p.starts_with("aether:"));
            assert!(p.ends_with(':'));
        }
    }
}
