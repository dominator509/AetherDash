/**
 * Session management for AETHER Terminal.
 *
 * Handles token-based authentication against the gateway's
 * POST /auth/validate endpoint, with fail-closed keychain persistence.
 *
 * Session state is NEVER assigned before
 * keychain persistence succeeds (blocker #4 fix).
 *
 * Credential issuance, TOTP enrollment, and login forms belong
 * to EP-401 (Decision Log 2026-07-12).
 */

import { tauriSetSessionToken, tauriGetSessionToken, tauriClearSessionToken } from "./tauri";

// ── Types ────────────────────────────────────────────────────────────────────

export type SessionState = "unauthenticated" | "authenticating" | "authenticated" | "expired";

// ── Internal state ───────────────────────────────────────────────────────────

let sessionState: SessionState = "unauthenticated";
let currentUsername: string | null = null;

const stateCallbacks: Array<(state: SessionState) => void> = [];

function setSessionState(state: SessionState): void {
  sessionState = state;
  for (const cb of stateCallbacks) cb(state);
}

// ── Public callbacks ─────────────────────────────────────────────────────────

export function onSessionStateChange(cb: (state: SessionState) => void): () => void {
  stateCallbacks.push(cb);
  return () => {
    const idx = stateCallbacks.indexOf(cb);
    if (idx >= 0) stateCallbacks.splice(idx, 1);
  };
}

// ── Keychain helpers ─────────────────────────────────────────────────────────

/**
 * Retrieve the stored session token from the OS keychain
 * (or in-memory fallback outside Tauri).
 */
export async function getToken(): Promise<string | null> {
  return await tauriGetSessionToken();
}

/**
 * Get the current session state.
 */
export function getSessionState(): SessionState {
  return sessionState;
}

/**
 * Get the current username / actor ID (if authenticated).
 */
export function getUsername(): string | null {
  return currentUsername;
}

// ── URL helpers ──────────────────────────────────────────────────────────────

let cachedGatewayUrl: string | null = null;

/**
 * Read the gateway URL from Tauri persisted config, falling back to the
 * build-time env var or the localhost default. Must be called once during
 * bootstrap before login/validateToken are used.
 *
 * This is the authoritative gateway URL — all components (login, validate,
 * WS connect) read it via getGatewayUrl().
 */
export async function initGatewayUrl(): Promise<string> {
  if (cachedGatewayUrl) return cachedGatewayUrl;
  const { tauriGetGatewayUrl } = await import("./tauri");
  cachedGatewayUrl = await tauriGetGatewayUrl();
  return cachedGatewayUrl;
}

/**
 * Return the authoritative cached gateway URL.
 * initGatewayUrl() must be called first (bootstrap does this).
 */
export function getGatewayUrl(): string {
  return cachedGatewayUrl ?? "ws://localhost:8080/ws";
}

/**
 * Convert a WS/WSS gateway URL to an HTTP/HTTPS auth base URL.
 * e.g. ws://localhost:8080/ws -> http://localhost:8080/auth/validate
 */
function authUrlFromGateway(gatewayUrl: string): string {
  return gatewayUrl.replace(/^ws:/, "http:").replace(/\/ws$/, "/auth/validate");
}

// ── Login / Logout ───────────────────────────────────────────────────────────

/**
 * Authenticate with the gateway using a session token.
 *
 * Fail-closed flow:
 * 1. POST token to gateway /auth/validate endpoint
 * 2. On valid response, persist token to Tauri keychain
 * 3. ONLY after keychain succeeds: update in-memory state
 * 4. If keychain fails: clear provisional state, throw error
 *
 * @param gatewayUrl - The gateway WebSocket URL
 * @param token      - The session token to validate and store
 */
export async function login(gatewayUrl: string, token: string): Promise<void> {
  if (sessionState === "authenticating") {
    throw new Error("Already authenticating");
  }
  if (sessionState === "authenticated") {
    return;
  }

  setSessionState("authenticating");

  try {
    if (!token || typeof token !== "string" || token.trim().length === 0) {
      throw new Error("Invalid token");
    }

    const authUrl = authUrlFromGateway(gatewayUrl);
    const response = await fetch(authUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ token: token.trim() }),
    });

    if (response.status === 401) {
      throw new Error("Invalid token");
    }

    if (!response.ok) {
      throw new Error("Gateway unreachable");
    }

    const body = (await response.json()) as { valid: boolean; actor_id?: string; tier?: number };
    if (!body.valid) {
      throw new Error("Invalid token");
    }

    // Step 2: Persist token to keychain BEFORE updating in-memory state
    try {
      await tauriSetSessionToken(token);
    } catch {
      throw new Error("Failed to store session token in keychain — authentication aborted");
    }

    // Step 3: Only after persistence succeeds, update in-memory state
    currentUsername = body.actor_id ?? null;
    setSessionState("authenticated");

    // Step 4: Open persistent WebSocket connection
    const { connect } = await import("./ws");
    connect(token, gatewayUrl);
  } catch (err) {
    // Fail-closed: clear any provisional state
    currentUsername = null;
    setSessionState("unauthenticated");
    throw err;
  }
}

/**
 * Log out and clear the stored session token from the OS keychain.
 */
export async function logout(): Promise<void> {
  const { disconnect } = await import("./ws");
  disconnect();

  try {
    await tauriClearSessionToken();
  } finally {
    currentUsername = null;
    setSessionState("unauthenticated");
  }
}

/**
 * Expire the session and clear the stored token.
 */
export async function expireSession(): Promise<void> {
  await logout();
  setSessionState("expired");
}

// ── Token Validation ─────────────────────────────────────────────────────────

/**
 * Validate a session token against the gateway's /auth/validate endpoint.
 * Uses the authoritative gateway URL from getGatewayUrl().
 *
 * @param token - The token to validate
 * @returns true if the gateway responds with valid: true
 */
export async function validateToken(token: string): Promise<boolean> {
  if (!token) return false;

  try {
    const baseUrl = getGatewayUrl();
    const authUrl = baseUrl.replace(/^ws:/, "http:").replace(/\/ws$/, "/auth/validate");

    const response = await fetch(authUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ token }),
    });

    if (response.status === 401) return false;
    if (!response.ok) return false;

    const body = (await response.json()) as { valid: boolean };
    return body.valid === true;
  } catch {
    return false;
  }
}

// ── Reset ────────────────────────────────────────────────────────────────────

/**
 * Reset all session state to unauthenticated.
 * Only clears in-memory state — keychain is not touched.
 */
export function reset(): void {
  currentUsername = null;
  setSessionState("unauthenticated");
}
