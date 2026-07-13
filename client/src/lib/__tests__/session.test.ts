import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { tauriSetSessionToken } from "../tauri";
import {
  getToken,
  login,
  logout,
  getSessionState,
  getGatewayUrl,
  getUsername,
  expireSession,
  validateToken,
  onSessionStateChange,
  reset,
} from "../session";
import { reset as wsReset, disconnect as wsDisconnect } from "../ws";
import { resetTauriFallback } from "../tauri";

// ── Mock ws — provide explicit overrides for validation ─────────────────────

vi.mock("../ws", () => ({
  connect: vi.fn(),
  disconnect: vi.fn(),
  getConnectionState: vi.fn(() => "disconnected"),
  getToken: vi.fn(() => null),
  send: vi.fn(() => false),
  backoffDelay: vi.fn(() => 200),
  reset: vi.fn(),
  onFrame: vi.fn(() => vi.fn()),
  onStateChange: vi.fn(() => vi.fn()),
  onError: vi.fn(() => vi.fn()),
  validateConnection: vi.fn(async (_token: string) => _token.length > 0),
}));

// ── Helpers ─────────────────────────────────────────────────────────────────

/**
 * Mock fetch to simulate a successful /auth/validate response.
 * Returns { valid: true, actor_id } by default.
 */
function mockFetchValid(overrides?: { valid?: boolean; actor_id?: string; status?: number }) {
  const { valid = true, actor_id = "test-user", status = 200 } = overrides ?? {};
  globalThis.fetch = vi.fn().mockResolvedValue({
    ok: status >= 200 && status < 300,
    status,
    statusText: status === 401 ? "Unauthorized" : "OK",
    json: () => Promise.resolve({ valid, actor_id }),
  });
}

/** Mock fetch to simulate a network error (gateway unreachable). */
function mockFetchNetworkError() {
  globalThis.fetch = vi.fn().mockRejectedValue(new TypeError("fetch failed"));
}

describe("session", () => {
  beforeEach(() => {
    reset();
    wsReset();
    resetTauriFallback();
    // Default mock: /auth/validate returns valid: true
    mockFetchValid();
  });

  afterEach(() => {
    reset();
    wsReset();
    resetTauriFallback();
    vi.restoreAllMocks();
  });

  // ── Original tests (preserved) ──────────────────────────────────────────

  it("should store and retrieve a token in memory", async () => {
    await tauriSetSessionToken("test-token");
    expect(await getToken()).toBe("test-token");
  });

  it("should clear token on logout", async () => {
    await tauriSetSessionToken("test-token");
    await logout();
    expect(await getToken()).toBeNull();
  });

  it("should set token on login", async () => {
    await login("ws://localhost:8080/ws", "test-session-token");
    expect(await getToken()).toBeTruthy();
  });

  it("should return null when no token is set", async () => {
    expect(await getToken()).toBeNull();
  });

  // ── Session state transitions ───────────────────────────────────────────

  it("starts in unauthenticated state", () => {
    expect(getSessionState()).toBe("unauthenticated");
  });

  it("transitions through authenticating to authenticated during login", async () => {
    const states: string[] = [];
    const unsub = onSessionStateChange((s) => states.push(s));

    await login("ws://localhost:8080/ws", "test-token");
    expect(states).toContain("authenticating");
    expect(states).toContain("authenticated");
    expect(getSessionState()).toBe("authenticated");

    unsub();
  });

  it("transitions to authenticated after successful login", async () => {
    await login("ws://localhost:8080/ws", "test-token");
    expect(getSessionState()).toBe("authenticated");
  });

  it("transitions to unauthenticated after logout", async () => {
    await login("ws://localhost:8080/ws", "test-token");
    expect(getSessionState()).toBe("authenticated");

    await logout();
    expect(getSessionState()).toBe("unauthenticated");
  });

  it("transitions to expired on expireSession", async () => {
    await login("ws://localhost:8080/ws", "test-token");
    await expireSession();
    expect(getSessionState()).toBe("expired");
    expect(await getToken()).toBeNull();
  });

  it("notifies session state change callbacks", async () => {
    const states: string[] = [];
    onSessionStateChange((s) => states.push(s));

    await login("ws://localhost:8080/ws", "test-token");
    expect(states).toContain("authenticated");

    await logout();
    expect(states).toContain("unauthenticated");
  });

  it("is a no-op to login when already authenticated", async () => {
    await login("ws://localhost:8080/ws", "test-token");
    const token = await getToken();

    await login("ws://localhost:8080/ws", "another-token");
    // Token should be the same as before (login was no-op)
    expect(await getToken()).toBe(token);
  });

  it("is a no-op to login when already authenticated (synchronous login completes before second call)", async () => {
    await login("ws://localhost:8080/ws", "test-token");
    await expect(login("ws://localhost:8080/ws", "another-token")).resolves.toBeUndefined();
  });

  // ── Login details ───────────────────────────────────────────────────────

  it("stores gateway URL after login", async () => {
    await login("ws://localhost:8080/ws", "test-token");
    expect(getGatewayUrl()).toBe("ws://localhost:8080/ws");
  });

  it("stores username/actor_id after login", async () => {
    mockFetchValid({ actor_id: "alice" });
    await login("ws://localhost:8080/ws", "test-token");
    expect(getUsername()).toBe("alice");
  });

  it("clears username on logout", async () => {
    await login("ws://localhost:8080/ws", "test-token");
    await logout();
    expect(getUsername()).toBeNull();
  });

  it("preserves gateway URL on logout (URL is config, not session)", async () => {
    await login("ws://localhost:8080/ws", "test-token");
    await logout();
    expect(getGatewayUrl()).toBe("ws://localhost:8080/ws");
  });

  // ── Token validation ────────────────────────────────────────────────────

  it("validateToken returns true for valid token", async () => {
    mockFetchValid();
    const valid = await validateToken("some-valid-token");
    expect(valid).toBe(true);
  });

  it("validateToken returns false for empty token", async () => {
    const valid = await validateToken("");
    expect(valid).toBe(false);
  });

  it("validateToken returns false when gateway returns invalid", async () => {
    mockFetchValid({ valid: false });
    const valid = await validateToken("invalid-token");
    expect(valid).toBe(false);
  });

  it("validateToken returns false on network error", async () => {
    mockFetchNetworkError();
    const valid = await validateToken("some-token");
    expect(valid).toBe(false);
  });

  // ── Logout clears state ─────────────────────────────────────────────────

  it("logout clears token and username, preserves gateway URL", async () => {
    await login("ws://localhost:8080/ws", "test-token");
    expect(await getToken()).toBeTruthy();
    expect(getGatewayUrl()).toBe("ws://localhost:8080/ws");

    await logout();

    expect(await getToken()).toBeNull();
    expect(getUsername()).toBeNull();
    expect(getGatewayUrl()).toBe("ws://localhost:8080/ws");
  });

  it("logout disconnects WebSocket", async () => {
    await login("ws://localhost:8080/ws", "test-token");
    await logout();
    expect(vi.mocked(wsDisconnect)).toHaveBeenCalledTimes(1);
  });

  // ── Edge cases ──────────────────────────────────────────────────────────

  it("multiple logouts are safe", async () => {
    await login("ws://localhost:8080/ws", "test-token");
    await logout();
    await logout(); // Second logout
    expect(getSessionState()).toBe("unauthenticated");
  });

  it("expireSession resets token but keeps session state as expired", async () => {
    await tauriSetSessionToken("test-token");
    await expireSession();
    expect(await getToken()).toBeNull();
    expect(getSessionState()).toBe("expired");
  });

  it("reset clears in-memory session state, keychain is untouched", async () => {
    await login("ws://localhost:8080/ws", "test-token");
    reset();
    expect(getSessionState()).toBe("unauthenticated");
    expect(await getToken()).toBeTruthy(); // keychain not cleared by reset
  });

  // ── Auth error handling ─────────────────────────────────────────────────

  it("throws on 401 response", async () => {
    mockFetchValid({ valid: false, status: 401 });
    await expect(login("ws://localhost:8080/ws", "bad-token")).rejects.toThrow("Invalid token");
    expect(getSessionState()).toBe("unauthenticated");
  });

  it("throws when gateway returns valid: false", async () => {
    mockFetchValid({ valid: false, status: 200 });
    await expect(login("ws://localhost:8080/ws", "bad-token")).rejects.toThrow("Invalid token");
    expect(getSessionState()).toBe("unauthenticated");
  });

  it("throws when gateway auth endpoint is unreachable", async () => {
    mockFetchNetworkError();
    await expect(login("ws://localhost:8080/ws", "test-token")).rejects.toThrow("fetch failed");
    expect(getSessionState()).toBe("unauthenticated");
  });

  it("throws on empty token", async () => {
    await expect(login("ws://localhost:8080/ws", "")).rejects.toThrow("Invalid token");
    expect(getSessionState()).toBe("unauthenticated");
  });

  // ── Token persistence via Tauri fallback ────────────────────────────────

  it("Tauri fallback persists token across module state resets", async () => {
    // This simulates what happens when the bootstrap flow reads
    // a previously-persisted token after a page reload
    await login("ws://localhost:8080/ws", "test-token");

    // Verify the Tauri fallback has the token
    const { tauriGetSessionToken } = await import("../tauri");
    const stored = await tauriGetSessionToken();
    expect(stored).toBeTruthy();

    // After a full reset, the in-memory state is cleared
    // but the keychain token persists (reset is memory-only)
    reset();
    expect(getSessionState()).toBe("unauthenticated");
    expect(await getToken()).toBeTruthy();
  });
});
