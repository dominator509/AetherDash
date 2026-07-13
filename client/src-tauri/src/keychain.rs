// Keychain module for session token storage.
//
// Real OS keychain integration via the `keyring` crate.
// Uses the platform-native credential store:
//   - Windows: Windows Credential Manager (wincred)
//   - macOS: Keychain Services
//   - Linux: Secret Service / kernel keyring
//
// ADR-0008: The Rust shell (not the web layer) owns secrets.
// The web layer accesses tokens exclusively through these Tauri commands.
// ---------------------------------------------------------------------------
// Platform abstraction: real keyring in production, in-memory in tests
// ---------------------------------------------------------------------------

/// Retrieve the stored session token, if any.
///
/// Returns `Ok(None)` when no token is stored (first launch or after logout).
/// Returns `Ok(Some(token))` when a token is available.
/// Returns `Err(msg)` when the OS keychain is unavailable or the read fails.
#[tauri::command]
pub fn get_session_token() -> Result<Option<String>, String> {
    get_token_impl()
}

/// Persist a session token to the OS keychain.
///
/// Overwrites any previously stored token.
/// The token is a JWT-like string returned by the gateway after authentication.
#[tauri::command]
pub fn set_session_token(token: String) -> Result<(), String> {
    set_token_impl(&token)
}

/// Delete the stored session token from the OS keychain (logout / expiry).
///
/// Idempotent: safe to call when no token is stored.
#[tauri::command]
pub fn delete_session_token() -> Result<(), String> {
    delete_token_impl()
}

// ---------------------------------------------------------------------------
// Implementation selection
// ---------------------------------------------------------------------------

/// Production implementation backed by the OS keychain.
#[cfg(not(test))]
mod imp {
    use keyring::Entry;

    const SERVICE_NAME: &str = "aether-terminal";
    const TOKEN_KEY: &str = "session-token";

    pub fn get() -> Result<Option<String>, String> {
        let entry = Entry::new(SERVICE_NAME, TOKEN_KEY)
            .map_err(|e| format!("Keychain unavailable: {}", e))?;
        match entry.get_password() {
            Ok(token) => Ok(Some(token)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(format!("Failed to read session token: {}", e)),
        }
    }

    pub fn set(token: &str) -> Result<(), String> {
        let entry = Entry::new(SERVICE_NAME, TOKEN_KEY)
            .map_err(|e| format!("Keychain unavailable: {}", e))?;
        entry.set_password(token).map_err(|e| format!("Failed to store session token: {}", e))
    }

    pub fn delete() -> Result<(), String> {
        let entry = Entry::new(SERVICE_NAME, TOKEN_KEY)
            .map_err(|e| format!("Keychain unavailable: {}", e))?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(format!("Failed to delete session token: {}", e)),
        }
    }
}

/// Test implementation backed by an in-memory Mutex so tests are hermetic
/// and do not depend on the platform credential store.
#[cfg(test)]
mod imp {
    use std::sync::Mutex;

    static TOKEN: Mutex<Option<String>> = Mutex::new(None);

    pub fn get() -> Result<Option<String>, String> {
        TOKEN.lock().map(|g| g.clone()).map_err(|e| format!("Lock error: {}", e))
    }

    pub fn set(token: &str) -> Result<(), String> {
        TOKEN
            .lock()
            .map(|mut g| *g = Some(token.to_string()))
            .map_err(|e| format!("Lock error: {}", e))
    }

    pub fn delete() -> Result<(), String> {
        TOKEN.lock().map(|mut g| *g = None).map_err(|e| format!("Lock error: {}", e))
    }
}

fn get_token_impl() -> Result<Option<String>, String> {
    imp::get()
}

fn set_token_impl(token: &str) -> Result<(), String> {
    imp::set(token)
}

fn delete_token_impl() -> Result<(), String> {
    imp::delete()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Test the command functions against the in-memory implementation.
    /// The #[cfg(test)] implementation uses a Mutex, so these are hermetic.

    #[test]
    fn test_get_returns_none_initially() {
        // Ensure clean state
        let _ = delete_token_impl();
        let result = get_token_impl();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }

    #[test]
    fn test_set_and_get() {
        let _ = delete_token_impl();
        set_token_impl("test-token-123").unwrap();
        let result = get_token_impl().unwrap();
        assert_eq!(result, Some("test-token-123".to_string()));
    }

    #[test]
    fn test_overwrite() {
        let _ = delete_token_impl();
        set_token_impl("first-token").unwrap();
        set_token_impl("second-token").unwrap();
        let result = get_token_impl().unwrap();
        assert_eq!(result, Some("second-token".to_string()));
    }

    #[test]
    fn test_delete_clears() {
        let _ = delete_token_impl();
        set_token_impl("temp").unwrap();
        delete_token_impl().unwrap();
        let result = get_token_impl().unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_delete_is_idempotent() {
        let _ = delete_token_impl();
        delete_token_impl().unwrap(); // Second delete on already-empty
        let result = get_token_impl().unwrap();
        assert_eq!(result, None);
    }
}
