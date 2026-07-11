use std::fmt;

/// Auth stub — validates token format, stamps origin.
/// Full session lookup and tier enforcement: EP-401.
#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub actor_id: String,
    pub tier: u8,
    pub origin: OriginInfo,
}

#[derive(Debug, Clone)]
pub struct OriginInfo {
    pub kind: String, // "user", "agent", "automation"
    pub actor_id: String,
}

/// Validate a bearer token format. Stub: accepts "test-" prefixed tokens in debug builds.
/// Real implementation (EP-401): query sessions table, verify expiry, check grants.
pub fn validate_token(token: Option<&str>) -> Result<SessionInfo, AuthError> {
    let token = token.ok_or(AuthError::MissingToken)?;
    let token = token.strip_prefix("Bearer ").unwrap_or(token);
    // Test tokens: ONLY available in debug builds (cfg(debug_assertions)).
    // Production builds: all tokens must validate against the sessions table (EP-401).
    if cfg!(debug_assertions) && token.starts_with("test-") {
        let actor_id = token.trim_start_matches("test-").to_string();
        Ok(SessionInfo {
            actor_id: actor_id.clone(),
            tier: 3, // Stub: tier 3 for test tokens (can access paper)
            origin: OriginInfo { kind: "user".into(), actor_id },
        })
    } else {
        // EP-401: real validation — query sessions table
        Err(AuthError::InvalidToken(token.into()))
    }
}

#[derive(Debug)]
pub enum AuthError {
    MissingToken,
    InvalidToken(String),
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuthError::MissingToken => write!(f, "missing token"),
            AuthError::InvalidToken(_token) => {
                // _token intentionally not disclosed per SPEC-006.
                // Field is stored for EP-401 logging/debugging.
                let _ = _token;
                write!(f, "invalid token")
            }
        }
    }
}

impl std::error::Error for AuthError {}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_token_accepted() {
        let session = validate_token(Some("Bearer test-alice")).unwrap();
        assert_eq!(session.actor_id, "alice");
        assert_eq!(session.origin.actor_id, "alice");
    }
    #[test]
    fn missing_token_rejected() {
        assert!(validate_token(None).is_err());
    }
    #[test]
    fn bad_token_rejected() {
        assert!(validate_token(Some("Bearer bad-token")).is_err());
    }
    #[test]
    fn error_display_does_not_leak_token() {
        let err = AuthError::InvalidToken("super-secret-token".into());
        let msg = err.to_string();
        assert_eq!(msg, "invalid token");
        assert!(!msg.contains("super-secret"));
    }
}
