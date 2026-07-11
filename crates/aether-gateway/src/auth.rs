/// Auth stub — validates token format, stamps origin.
/// Full session lookup and tier enforcement: EP-401.
pub struct SessionInfo {
    pub actor_id: String,
    pub tier: u8,
    pub origin: OriginInfo,
}

pub struct OriginInfo {
    pub kind: String,  // "user", "agent", "automation"
    pub actor_id: String,
}

/// Validate a bearer token format. Stub: accepts "test-" prefixed tokens.
/// Real implementation (EP-401): query sessions table, verify expiry, check grants.
pub fn validate_token(token: Option<&str>) -> Result<SessionInfo, AuthError> {
    let token = token.ok_or(AuthError::MissingToken)?;
    let token = token.strip_prefix("Bearer ").unwrap_or(token);
    if token.starts_with("test-") {
        Ok(SessionInfo {
            actor_id: token.trim_start_matches("test-").to_string(),
            tier: 3, // Stub: tier 3 for test tokens (can access paper)
            origin: OriginInfo {
                kind: "user".into(),
                actor_id: token.to_string(),
            },
        })
    } else {
        // EP-401: real validation — query sessions table
        Err(AuthError::InvalidToken(token.into()))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("missing token")]
    MissingToken,
    #[error("invalid token: {0}")]
    InvalidToken(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_token_accepted() {
        assert!(validate_token(Some("Bearer test-alice")).is_ok());
    }
    #[test]
    fn missing_token_rejected() {
        assert!(validate_token(None).is_err());
    }
    #[test]
    fn bad_token_rejected() {
        assert!(validate_token(Some("Bearer bad-token")).is_err());
    }
}
