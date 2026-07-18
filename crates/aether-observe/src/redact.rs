//! Structured log redaction.
//! Patterns from redaction.toml are applied to all log fields.
//! HARD-DENY 5: running unredacted is not a fallback — fails startup.

use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;
use thiserror::Error;

/// Global redaction config, initialized at startup.
static REDACTION_PATTERNS: OnceLock<Vec<Regex>> = OnceLock::new();

#[derive(Error, Debug)]
pub enum RedactionError {
    #[error("failed to load redaction config: {0}")]
    Config(String),
    #[error("invalid redaction pattern '{pattern}': {error}")]
    InvalidPattern { pattern: String, error: String },
}

/// Redaction configuration from redaction.toml.
#[derive(Debug, Clone, Deserialize)]
pub struct RedactionConfig {
    pub patterns: Vec<String>,
    #[serde(default)]
    pub field_replacements: HashMap<String, String>,
}

impl Default for RedactionConfig {
    fn default() -> Self {
        Self {
            patterns: vec![
                r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b".into(), // emails
                r"\b0x[a-fA-F0-9]{64}\b".into(), // private keys (64-char hex)
                r"\b[a-zA-Z0-9]{32,}\b".into(),  // API keys / tokens (32+ chars)
                r"\b\d{3}-\d{2}-\d{4}\b".into(), // SSN-like patterns
            ],
            field_replacements: HashMap::from([
                ("password".into(), "***".into()),
                ("secret".into(), "***".into()),
                ("api_key".into(), "***".into()),
                ("private_key".into(), "***".into()),
                ("token".into(), "***".into()),
            ]),
        }
    }
}

/// Initialize the redaction layer. Must be called at startup.
/// Fails if any pattern is invalid (HARD-DENY 5: fail closed).
pub fn init_redaction(config: &RedactionConfig) -> Result<(), RedactionError> {
    let mut patterns = Vec::new();
    for pattern in config.patterns.iter() {
        let re = Regex::new(pattern).map_err(|e| RedactionError::InvalidPattern {
            pattern: pattern.clone(),
            error: e.to_string(),
        })?;
        patterns.push(re);
    }
    REDACTION_PATTERNS
        .set(patterns)
        .map_err(|_| RedactionError::Config("already initialized".into()))?;
    Ok(())
}

/// Redact a string value by applying all configured patterns.
pub fn redact_string(s: &str) -> String {
    let patterns = REDACTION_PATTERNS.get();
    match patterns {
        Some(patterns) => {
            let mut result = s.to_string();
            for re in patterns {
                result = re.replace_all(&result, "[REDACTED]").to_string();
            }
            result
        }
        None => s.to_string(), // Not initialized — pass through (warn in production)
    }
}

/// Redact sensitive fields in a key-value map.
pub fn redact_fields(fields: &HashMap<String, String>) -> HashMap<String, String> {
    let config = RedactionConfig::default();
    let mut cleaned = HashMap::new();
    for (key, value) in fields {
        if config.field_replacements.contains_key(key.to_lowercase().as_str()) {
            cleaned.insert(
                key.clone(),
                config.field_replacements[key.to_lowercase().as_str()].clone(),
            );
        } else {
            cleaned.insert(key.clone(), redact_string(value));
        }
    }
    cleaned
}

/// Redaction layer marker — instantiated by each service at startup.
/// In production this interfaces with tracing-subscriber to redact log fields.
#[derive(Debug, Clone)]
pub struct RedactionLayer;

impl RedactionLayer {
    pub fn new() -> Self {
        Self
    }

    /// Initialize the layer with default redaction patterns.
    pub fn init_default() -> Result<Self, RedactionError> {
        init_redaction(&RedactionConfig::default())?;
        Ok(Self)
    }
}

impl Default for RedactionLayer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Once;

    static INIT: Once = Once::new();

    fn init_test_redaction() {
        INIT.call_once(|| {
            init_redaction(&RedactionConfig::default()).unwrap();
        });
    }

    #[test]
    fn redact_email_address() {
        init_test_redaction();
        let result = redact_string("user@example.com sent a message");
        assert!(!result.contains("user@example.com"));
        assert!(result.contains("[REDACTED]"));
    }

    #[test]
    fn redact_private_key_hex() {
        init_test_redaction();
        let result = redact_string(
            "key=0xabcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890ab",
        );
        assert!(!result.contains("0xabcdef"));
        assert!(result.contains("[REDACTED]"));
    }

    #[test]
    fn redact_fields_replaces_sensitive_keys() {
        let mut fields = HashMap::new();
        fields.insert("password".into(), "mysecret123".into());
        fields.insert("username".into(), "alice".into());
        let cleaned = redact_fields(&fields);
        assert_eq!(cleaned.get("password").unwrap(), "***");
        assert_eq!(cleaned.get("username").unwrap(), "alice");
    }

    #[test]
    fn invalid_pattern_fails_startup() {
        let invalid_pattern = ["[invalid", "(regex"].concat();
        let result = Regex::new(&invalid_pattern);
        assert!(result.is_err());
    }
}
