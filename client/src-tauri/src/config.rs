/// Gateway configuration module.
///
/// Provides Tauri commands to read and write the gateway WebSocket URL.
/// The URL is persisted in the Tauri store (`config.json`) and can be
/// overridden at launch via the `AETHER_CLIENT__GATEWAY_URL` environment
/// variable (which is also exposed to Vite builds by `vite.config.ts`).
///
/// Blocker #5: set_gateway_url validates that the URL's host is localhost
/// or 127.0.0.1 to keep CSP and configuration in sync.
use serde_json::Value;
use tauri_plugin_store::StoreExt;

/// Default gateway URL used when no other source provides a value.
const DEFAULT_GATEWAY_URL: &str = "ws://localhost:8080/ws";

/// Validate a gateway URL's host is localhost or 127.0.0.1.
/// Returns `Some(url)` if valid, `None` otherwise.
fn validate_localhost_url(url: &str) -> Option<&str> {
    let host = extract_host(url)?;
    if host == "localhost" || host == "127.0.0.1" {
        Some(url)
    } else {
        None
    }
}

/// Read the current gateway WebSocket URL.
///
/// Resolution order (first valid localhost URL wins):
/// 1. `AETHER_CLIENT__GATEWAY_URL` environment variable (validated as localhost)
/// 2. Persisted value in the Tauri store (`config.json`)
/// 3. `ws://localhost:8080/ws` (fallback)
///
/// Non-localhost URLs from any source are silently skipped in favour of
/// the next source, keeping CSP connect-src in sync with configuration.
#[tauri::command]
pub fn get_gateway_url(app: tauri::AppHandle) -> String {
    // 1. Env var override (validated — same policy as set_gateway_url)
    if let Ok(url) = std::env::var("AETHER_CLIENT__GATEWAY_URL") {
        if let Some(valid) = validate_localhost_url(&url) {
            return valid.to_string();
        }
    }

    // 2. Persisted store value (validated)
    if let Ok(store) = app.store("config.json") {
        if let Some(Value::String(url)) = store.get("gateway_url") {
            if !url.is_empty() {
                if let Some(valid) = validate_localhost_url(url.as_str()) {
                    return valid.to_string();
                }
            }
        }
    }

    DEFAULT_GATEWAY_URL.to_string()
}

/// Extract the hostname portion of a URL string.
///
/// Handles URLs of the form `scheme://host[:port][/path]`.
/// Returns `None` if the URL cannot be parsed.
fn extract_host(url: &str) -> Option<&str> {
    // Find the scheme separator `://`
    let after_scheme = url.find("://")?;
    let after_scheme = &url[after_scheme + 3..];

    // Host ends at the next `:`, `/`, or end of string
    let host_end = after_scheme.find([':', '/']).unwrap_or(after_scheme.len());

    Some(&after_scheme[..host_end])
}

/// Persist a new gateway WebSocket URL to the Tauri store.
///
/// Validates that the URL's host is `localhost` or `127.0.0.1` (any port).
/// Returns an error for non-localhost hosts to keep CSP and configuration in sync.
#[tauri::command]
pub fn set_gateway_url(app: tauri::AppHandle, url: String) -> Result<(), String> {
    // Validate the URL is localhost or 127.0.0.1 (any port)
    let host = extract_host(&url).ok_or_else(|| "Invalid URL format".to_string())?;
    if host != "localhost" && host != "127.0.0.1" {
        return Err("Only localhost gateway URLs are supported in this phase".to_string());
    }

    let store =
        app.store("config.json").map_err(|e| format!("Failed to open config store: {}", e))?;
    store.set("gateway_url", Value::String(url));
    store.save().map_err(|e| format!("Failed to persist gateway URL: {}", e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_host_localhost() {
        assert_eq!(extract_host("ws://localhost:8080/ws"), Some("localhost"));
    }

    #[test]
    fn test_extract_host_localhost_no_port() {
        assert_eq!(extract_host("ws://localhost/ws"), Some("localhost"));
    }

    #[test]
    fn test_extract_host_localhost_no_path() {
        assert_eq!(extract_host("ws://localhost:8080"), Some("localhost"));
    }

    #[test]
    fn test_extract_host_ipv4_loopback() {
        assert_eq!(extract_host("ws://127.0.0.1:8080/ws"), Some("127.0.0.1"));
    }

    #[test]
    fn test_extract_host_https() {
        assert_eq!(extract_host("https://localhost:8443/ws"), Some("localhost"),);
    }

    #[test]
    fn test_extract_host_external_rejected() {
        assert_eq!(extract_host("ws://example.com:8080/ws"), Some("example.com"));
    }

    #[test]
    fn test_extract_host_invalid() {
        assert_eq!(extract_host("not-a-url"), None);
    }

    #[test]
    fn test_extract_host_empty() {
        assert_eq!(extract_host(""), None);
    }
}
