use std::fmt;

use sqlx::PgPool;

/// Session information returned after successful token validation.
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

/// Initialize a connection pool to the gateway's Postgres sessions database.
///
/// # Panics
/// Panics if the pool cannot be created (e.g., invalid URL).
#[allow(clippy::expect_used)]
pub fn init_db_pool(database_url: &str) -> PgPool {
    PgPool::connect_lazy(database_url).expect("failed to create Postgres connection pool")
}

/// Validate a bearer token and return session info.
///
/// Validation order:
/// 1. **Test tokens** (`test-` prefix) — only in debug builds (`cfg(debug_assertions)`).
///    These bypass the DB and exist so unit tests and dev workflows never need Postgres.
/// 2. **DB lookup** — when a `PgPool` is provided, query the `sessions` table.
/// 3. **Fail** — if neither path matches.
///
/// Passing `pool: None` is equivalent to "no DB available" and causes the function
/// to rely solely on test tokens (debug) or fail outright (release).
pub async fn validate_token(
    pool: Option<&PgPool>,
    token: Option<&str>,
) -> Result<SessionInfo, AuthError> {
    let token = token.ok_or(AuthError::MissingToken)?;
    let token = token.strip_prefix("Bearer ").unwrap_or(token);

    // Test tokens: ONLY available in debug builds (cfg(debug_assertions)).
    // Production builds: all tokens must validate against the sessions table.
    if cfg!(debug_assertions) && token.starts_with("test-") {
        let actor_id = token.trim_start_matches("test-").to_string();
        return Ok(SessionInfo {
            actor_id: actor_id.clone(),
            tier: 3, // Stub: tier 3 for test tokens (can access paper)
            origin: OriginInfo { kind: "user".into(), actor_id },
        });
    }

    // DB lookup if a pool was provided
    if let Some(pool) = pool {
        let row = sqlx::query_as::<_, (String, i32, String)>(
            "SELECT actor_id, tier, origin_kind FROM sessions WHERE actor_id = $1",
        )
        .bind(token)
        .fetch_optional(pool)
        .await
        .map_err(|e| AuthError::DbError(e.to_string()))?;

        if let Some((actor_id, tier, origin_kind)) = row {
            let aid = actor_id.clone();
            return Ok(SessionInfo {
                actor_id: aid,
                tier: tier as u8,
                origin: OriginInfo { kind: origin_kind, actor_id },
            });
        }
    }

    Err(AuthError::InvalidToken(token.into()))
}

#[derive(Debug)]
pub enum AuthError {
    MissingToken,
    InvalidToken(String),
    SessionNotFound(String),
    DbError(String),
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
            AuthError::SessionNotFound(id) => write!(f, "session not found: {id}"),
            AuthError::DbError(_) => write!(f, "database error"),
        }
    }
}

impl std::error::Error for AuthError {}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: call validate_token with no pool (test-token only path).
    async fn validate(token: Option<&str>) -> Result<SessionInfo, AuthError> {
        validate_token(None, token).await
    }

    #[tokio::test]
    async fn test_token_accepted() {
        let session = validate(Some("Bearer test-alice")).await.unwrap();
        assert_eq!(session.actor_id, "alice");
        assert_eq!(session.origin.actor_id, "alice");
    }

    #[tokio::test]
    async fn missing_token_rejected() {
        assert!(validate(None).await.is_err());
    }

    #[tokio::test]
    async fn bad_token_rejected() {
        assert!(validate(Some("Bearer bad-token")).await.is_err());
    }

    #[tokio::test]
    async fn error_display_does_not_leak_token() {
        let err = AuthError::InvalidToken("super-secret-token".into());
        let msg = err.to_string();
        assert_eq!(msg, "invalid token");
        assert!(!msg.contains("super-secret"));
    }

    #[tokio::test]
    async fn session_not_found_display() {
        let err = AuthError::SessionNotFound("unknown-id".into());
        assert_eq!(err.to_string(), "session not found: unknown-id");
    }

    #[tokio::test]
    async fn db_error_display() {
        let err = AuthError::DbError("connection refused".into());
        assert_eq!(err.to_string(), "database error");
    }
}
