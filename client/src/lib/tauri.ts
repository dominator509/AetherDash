/**
 * Tauri invoke wrapper for session token management.
 *
 * Abstracts the Tauri IPC layer so it can be mocked in tests
 * and gracefully degrades to in-memory storage when running
 * outside a Tauri webview (e.g. browser dev mode, test runner).
 *
 * Three operations: set, get, clear — all backed by the Tauri
 * keychain plugin in production.
 *
 * BOLT-004 / blocker #3: Under Tauri the keychain is the only
 * durable store. If invoke fails under Tauri the error propagates
 * (fail-closed). In-memory fallback is ONLY used when NOT running
 * under Tauri (browser dev, test runner).
 */

// In-memory fallback used when Tauri IPC is unavailable
let fallbackToken: string | null = null;

/** True when running inside a Tauri webview (window.__TAURI_INTERNALS__ present). */
const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/**
 * Store a session token via the Tauri keychain command.
 * Propagates errors under Tauri (fail-closed).
 * Falls back to in-memory storage outside Tauri.
 */
export async function tauriSetSessionToken(token: string): Promise<void> {
  if (isTauri) {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("set_session_token", { token });
    return;
  }
  fallbackToken = token;
}

/**
 * Retrieve a session token from the Tauri keychain command.
 * Propagates errors under Tauri (fail-closed).
 * Falls back to in-memory storage outside Tauri.
 */
export async function tauriGetSessionToken(): Promise<string | null> {
  if (isTauri) {
    const { invoke } = await import("@tauri-apps/api/core");
    return await invoke("get_session_token");
  }
  return fallbackToken;
}

/**
 * Clear the session token from the Tauri keychain command.
 * Also clears the in-memory fallback.
 */
export async function tauriClearSessionToken(): Promise<void> {
  fallbackToken = null;
  if (isTauri) {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("delete_session_token");
  }
}

/**
 * Reset the in-memory fallback (for testing).
 */
export function resetTauriFallback(): void {
  fallbackToken = null;
}

// ── Gateway URL configuration ──────────────────────────────────────────

let fallbackGatewayUrl: string | null = null;

/**
 * Read the persisted gateway WebSocket URL via the Tauri config command.
 * Falls back to the build-time env var or default when outside Tauri.
 */
export async function tauriGetGatewayUrl(): Promise<string> {
  if (isTauri) {
    const { invoke } = await import("@tauri-apps/api/core");
    return await invoke<string>("get_gateway_url");
  }
  // Outside Tauri: use Vite env var or default
  return (
    import.meta.env.AETHER_CLIENT__GATEWAY_URL || fallbackGatewayUrl || "ws://localhost:8080/ws"
  );
}

/**
 * Persist a gateway URL via the Tauri config command.
 * Validated server-side to be localhost only.
 */
export async function tauriSetGatewayUrl(url: string): Promise<void> {
  if (isTauri) {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("set_gateway_url", { url });
    return;
  }
  fallbackGatewayUrl = url;
}
