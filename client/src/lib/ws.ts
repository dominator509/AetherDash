/**
 * WebSocket client for connecting to the AETHER gateway.
 *
 * Implements reconnect with exponential backoff and full jitter
 * per SPEC-006: base 200ms, cap 30s, max 5 attempts.
 *
 * Handles all 6 client→server and 10 server→client frame types
 * from SPEC-003. Unknown server frame types emit an error state
 * (never crash/throw).
 *
 * Token passes through web layer briefly during WS connect (required by
 * gateway's ?token= query param auth). Stored durably in OS keychain via
 * Tauri commands.
 */

import type { ServerFrame, ServerFrameType, ClientFrame, ErrorFrame } from "./frames";
import { SERVER_FRAME_TYPES } from "./frames";
import type { WebSocketLike } from "./websocket";
import { createWebSocket } from "./websocket";

// ── Constants ────────────────────────────────────────────────────────────────

export const DEFAULT_GATEWAY_URL = "ws://localhost:8080/ws";
const PING_INTERVAL_MS = 30_000;
const BACKOFF_BASE_MS = 200;
const BACKOFF_CAP_MS = 30_000;
const MAX_RECONNECT_ATTEMPTS = 5;

// ── Connection state ─────────────────────────────────────────────────────────

export type ConnectionState = "disconnected" | "connecting" | "connected" | "reconnecting";

// ── Internal state ───────────────────────────────────────────────────────────

interface ReconnectState {
  attempt: number;
  timer: ReturnType<typeof setTimeout> | null;
}

/** Token provider for reconnect — set by connect() as a closure over the token. */
let reconnectTokenProvider: (() => string | null) | null = null;

/** Gateway URL provider for reconnect — set by connect() alongside the token. */
let reconnectUrlProvider: (() => string | undefined) | null = null;

let ws: WebSocketLike | null = null;
const reconnectState: ReconnectState = { attempt: 0, timer: null };
let pingInterval: ReturnType<typeof setInterval> | null = null;
let connectionState: ConnectionState = "disconnected";

/** Callbacks for raw incoming frames. */
const frameCallbacks: Set<(frame: ServerFrame) => void> = new Set();

/** Callbacks for connection state changes. */
const stateCallbacks: Set<(state: ConnectionState) => void> = new Set();

/** Callbacks for errors (including unknown frame type errors). */
const errorCallbacks: Set<(error: ErrorFrame) => void> = new Set();

// ── State helpers ────────────────────────────────────────────────────────────

function setConnectionState(newState: ConnectionState): void {
  connectionState = newState;
  for (const cb of stateCallbacks) {
    try {
      cb(newState);
    } catch {
      // Swallow callback errors to keep the WS loop alive
    }
  }
}

// ── Backoff ──────────────────────────────────────────────────────────────────

/**
 * Calculate backoff delay with full jitter per SPEC-006.
 * base: 200ms, cap: 30s, max 5 attempts.
 */
export function backoffDelay(attempt: number): number {
  if (attempt >= MAX_RECONNECT_ATTEMPTS) {
    return -1; // Signal to stop retrying
  }

  const exponential = Math.min(BACKOFF_CAP_MS, BACKOFF_BASE_MS * Math.pow(2, attempt));
  return Math.random() * exponential; // Full jitter, result in [0, exponential]
}

function scheduleReconnect(): void {
  if (reconnectState.timer !== null) {
    clearTimeout(reconnectState.timer);
  }

  const delay = backoffDelay(reconnectState.attempt);
  if (delay < 0) {
    console.warn(`[ws] Max reconnect attempts (${reconnectState.attempt}) reached. Giving up.`);
    setConnectionState("disconnected");
    return;
  }

  reconnectState.attempt++;
  setConnectionState("reconnecting");

  reconnectState.timer = setTimeout(() => {
    reconnectState.timer = null;
    const token = reconnectTokenProvider?.();
    if (token) {
      connectInternal(token, reconnectUrlProvider?.());
    }
  }, delay);
}

// ── Ping management ──────────────────────────────────────────────────────────

function startPing(): void {
  stopPing();
  pingInterval = setInterval(() => {
    sendRaw({ type: "ping" });
  }, PING_INTERVAL_MS);
}

function stopPing(): void {
  if (pingInterval !== null) {
    clearInterval(pingInterval);
    pingInterval = null;
  }
}

// ── Frame dispatch ───────────────────────────────────────────────────────────

/** Known server frame types indexed for fast lookup. */
const knownServerTypes = new Set<ServerFrameType>(SERVER_FRAME_TYPES);

function isServerFrameType(t: string): t is ServerFrameType {
  return knownServerTypes.has(t as ServerFrameType);
}

function handleMessage(event: MessageEvent): void {
  let parsed: Record<string, unknown>;
  try {
    parsed = JSON.parse(event.data as string);
  } catch {
    console.warn("[ws] Failed to parse incoming frame as JSON");
    return;
  }

  const frameType = parsed.type;
  if (typeof frameType !== "string" || frameType === "") {
    console.warn("[ws] Incoming frame missing/invalid 'type' field");
    return;
  }

  // Check for unknown server frame types per SPEC-003: emit error, never crash
  if (!isServerFrameType(frameType)) {
    const errorFrame: ErrorFrame = {
      type: "error",
      body: {
        code: "invalid_argument",
        message: `Unknown server frame type: "${frameType}"`,
        retryable: false,
        trace_id: (parsed.trace_id as string) ?? "",
        details: `The server sent a frame with unrecognized type "${frameType}". This may indicate a version mismatch.`,
      },
    };
    dispatchError(errorFrame);
    return;
  }

  const frame = parsed as unknown as ServerFrame;
  for (const cb of frameCallbacks) {
    try {
      cb(frame);
    } catch {
      // Swallow callback errors
    }
  }
}

function dispatchError(error: ErrorFrame): void {
  for (const cb of errorCallbacks) {
    try {
      cb(error);
    } catch {
      // Swallow callback errors
    }
  }
}

// ── WebSocket event handlers ─────────────────────────────────────────────────

function handleOpen(): void {
  reconnectState.attempt = 0;
  setConnectionState("connected");
  startPing();
}

function handleClose(): void {
  ws = null;
  stopPing();
  if (connectionState !== "disconnected") {
    scheduleReconnect();
  }
}

function handleError(): void {
  // The close event will fire after error, triggering reconnect
  ws?.close();
}

// ── Internal connect ─────────────────────────────────────────────────────────

function connectInternal(newToken: string, gatewayUrl?: string): void {
  if (ws && (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING)) {
    return;
  }

  setConnectionState("connecting");

  const baseUrl =
    gatewayUrl ??
    (import.meta.env.AETHER_CLIENT__GATEWAY_URL as string | undefined) ??
    DEFAULT_GATEWAY_URL;

  // Append token as query parameter per gateway auth spec
  const url = `${baseUrl}?token=${encodeURIComponent(newToken)}`;

  try {
    ws = createWebSocket(url) as WebSocketLike;

    ws.onopen = () => {
      handleOpen();
    };

    ws.onmessage = handleMessage;

    ws.onclose = () => {
      handleClose();
    };

    ws.onerror = () => {
      handleError();
    };
  } catch {
    setConnectionState("reconnecting");
    scheduleReconnect();
  }
}

// ── Token validation ─────────────────────────────────────────────────────────

/**
 * Open a brief WS connection to the gateway, send a ping, and wait for a pong.
 *
 * Used by `session.validateToken()` and during the login fallback path.
 * The connection is closed immediately after receiving (or timing out on) the pong.
 *
 * @param token      - The session token to validate
 * @param gatewayUrl - Optional gateway WS URL (defaults to DEFAULT_GATEWAY_URL)
 * @param timeoutMs  - Milliseconds to wait for a pong before giving up (default 5000)
 * @returns true if a pong was received within the timeout, false otherwise
 */
export async function validateConnection(
  token: string,
  gatewayUrl?: string,
  timeoutMs = 5_000,
): Promise<boolean> {
  return new Promise<boolean>((resolve) => {
    let settled = false;
    let timer: ReturnType<typeof setTimeout> | null = null;

    const baseUrl = gatewayUrl ?? DEFAULT_GATEWAY_URL;
    const url = `${baseUrl}?token=${encodeURIComponent(token)}`;

    let ws: WebSocketLike;
    try {
      ws = createWebSocket(url);
    } catch {
      resolve(false);
      return;
    }

    function finish(valid: boolean): void {
      if (settled) return;
      settled = true;
      if (timer !== null) clearTimeout(timer);
      try {
        ws.close();
      } catch {
        // Best-effort close
      }
      resolve(valid);
    }

    timer = setTimeout(() => finish(false), timeoutMs);

    ws.onopen = () => {
      try {
        ws.send(JSON.stringify({ type: "ping" }));
      } catch {
        finish(false);
      }
    };

    ws.onmessage = (event: MessageEvent) => {
      if (settled) return;
      try {
        const data = JSON.parse(event.data as string);
        if (data?.type === "pong") {
          finish(true);
        }
      } catch {
        // Ignore malformed messages during validation
      }
    };

    ws.onerror = () => finish(false);
    ws.onclose = () => finish(false);
  });
}

// ── Public API ───────────────────────────────────────────────────────────────

/**
 * Connect to the gateway with a session token.
 * If already connected, returns without reconnecting.
 *
 * The token is captured in a closure for reconnect but is NOT
 * stored as a module-level variable after this function returns.
 */
export function connect(newToken: string, gatewayUrl?: string): void {
  // Set token + URL providers for reconnect (captured in closure)
  reconnectTokenProvider = () => newToken;
  reconnectUrlProvider = () => gatewayUrl;
  // Reset attempt counter on explicit connect call
  reconnectState.attempt = 0;
  connectInternal(newToken, gatewayUrl);
}

/**
 * Disconnect from the gateway and cancel any pending reconnect.
 */
export function disconnect(): void {
  if (reconnectState.timer !== null) {
    clearTimeout(reconnectState.timer);
    reconnectState.timer = null;
  }
  reconnectState.attempt = 0;

  stopPing();

  if (ws) {
    ws.onclose = null; // Prevent reconnect on intentional close
    ws.close();
    ws = null;
  }

  reconnectTokenProvider = null;
  setConnectionState("disconnected");
}

/**
 * Send a typed client frame to the gateway.
 * Returns true if the frame was sent, false if not connected.
 */
export function send(frame: ClientFrame): boolean {
  if (!ws || ws.readyState !== WebSocket.OPEN) {
    console.warn("[ws] Cannot send — not connected");
    return false;
  }

  ws.send(JSON.stringify(frame));
  return true;
}

/**
 * Send a raw JSON-serializable object (for tests or low-level usage).
 */
function sendRaw(frame: Record<string, unknown>): void {
  if (!ws || ws.readyState !== WebSocket.OPEN) {
    return;
  }
  ws.send(JSON.stringify(frame));
}

/**
 * Register a callback for incoming server frames.
 * Returns an unsubscribe function.
 */
export function onFrame(callback: (frame: ServerFrame) => void): () => void {
  frameCallbacks.add(callback);
  return () => {
    frameCallbacks.delete(callback);
  };
}

/**
 * Register a callback for connection state changes.
 * Returns an unsubscribe function.
 */
export function onStateChange(callback: (state: ConnectionState) => void): () => void {
  stateCallbacks.add(callback);
  // Immediately notify with current state
  try {
    callback(connectionState);
  } catch {
    // Swallow
  }
  return () => {
    stateCallbacks.delete(callback);
  };
}

/**
 * Register a callback for error frames (including unknown frame type errors).
 * Returns an unsubscribe function.
 */
export function onError(callback: (error: ErrorFrame) => void): () => void {
  errorCallbacks.add(callback);
  return () => {
    errorCallbacks.delete(callback);
  };
}

/**
 * Get the current connection state.
 */
export function getConnectionState(): ConnectionState {
  return connectionState;
}

/**
 * Reset all state (primarily for testing).
 */
export function reset(): void {
  disconnect();
  frameCallbacks.clear();
  stateCallbacks.clear();
  errorCallbacks.clear();
}
