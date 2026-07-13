use std::fmt;

use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgPool;

/// Session information returned after successful token validation.
#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub session_id: String,
    pub actor_id: String,
    pub tier: u8,
    pub origin: OriginInfo,
    pub device_label: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OriginInfo {
    pub kind: String, // "human", "agent", "automation"
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
            session_id: "test-session".into(),
            actor_id: actor_id.clone(),
            tier: 3, // Stub: tier 3 for test tokens (can access paper)
            origin: OriginInfo { kind: "human".into(), actor_id },
            device_label: None,
        });
    }

    // DB lookup if a pool was provided
    if let Some(pool) = pool {
        // TODO(EP-401): upgrade to argon2id. SHA-256 is a temporary stand-in
        // for the EP-004 migration to ensure hashed-token authentication works.
        let hash = Sha256::digest(token.as_bytes());
        let token_hash: String = hash.iter().map(|b| format!("{:02x}", b)).collect();

        let row = sqlx::query_as::<_, (String, String, i32, String, Option<String>)>(
            "SELECT s.id, s.user_id, s.tier, s.origin_kind, s.device_label \
             FROM sessions s \
             WHERE s.token_hash = $1 AND s.expires_ts > now()",
        )
        .bind(&token_hash)
        .fetch_optional(pool)
        .await
        .map_err(|e| AuthError::DbError(e.to_string()))?;

        if let Some((session_id, user_id, tier, origin_kind, device_label)) = row {
            return Ok(SessionInfo {
                session_id,
                actor_id: user_id.clone(),
                tier: tier as u8,
                origin: OriginInfo { kind: origin_kind, actor_id: user_id },
                device_label,
            });
        }
    }

    Err(AuthError::InvalidToken(token.into()))
}

/// Request body for `POST /auth/validate`.
#[derive(Deserialize)]
pub struct ValidateRequest {
    pub token: String,
}

/// Response body for `POST /auth/validate`.
#[derive(Serialize)]
pub struct ValidateResponse {
    pub valid: bool,
    pub actor_id: Option<String>,
    pub tier: Option<u8>,
}

/// Handler for `POST /auth/validate`.
///
/// Accepts a JSON body with a `token` field and validates it against the
/// sessions database (or test-token logic in debug builds). Returns
/// `200 OK` with session metadata on success, `401 UNAUTHORIZED` on failure.
pub async fn validate_handler(
    State(state): State<crate::AppState>,
    Json(req): Json<ValidateRequest>,
) -> (StatusCode, Json<ValidateResponse>) {
    match validate_token(Some(&state.pool), Some(&req.token)).await {
        Ok(session) => (
            StatusCode::OK,
            Json(ValidateResponse {
                valid: true,
                actor_id: Some(session.actor_id),
                tier: Some(session.tier),
            }),
        ),
        Err(_) => (
            StatusCode::UNAUTHORIZED,
            Json(ValidateResponse { valid: false, actor_id: None, tier: None }),
        ),
    }
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

    /// Build a minimal SessionInfo for test use.
    #[allow(dead_code)]
    fn make_session(actor_id: &str, tier: u8, kind: &str) -> SessionInfo {
        SessionInfo {
            session_id: "test-session".into(),
            actor_id: actor_id.into(),
            tier,
            origin: OriginInfo { kind: kind.into(), actor_id: actor_id.into() },
            device_label: None,
        }
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
    async fn validate_handler_rejects_invalid_token() {
        let result = validate_token(None, Some("Bearer bad-token")).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn validate_handler_accepts_test_token() {
        let pool = init_db_pool("postgres://aether:aether@localhost:5432/aether");
        let state = crate::AppState::new(pool);
        let req = axum::Json(ValidateRequest { token: "test-alice".into() });
        let (status, body) = validate_handler(axum::extract::State(state), req).await;
        assert_eq!(status, axum::http::StatusCode::OK);
        assert!(body.valid);
        assert_eq!(body.actor_id.as_deref(), Some("alice"));
        assert_eq!(body.tier, Some(3));
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
