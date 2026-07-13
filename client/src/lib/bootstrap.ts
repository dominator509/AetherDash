/**
 * App bootstrap flow.
 *
 * Called once on mount from app.tsx. The bootstrap:
 * 1. Reads a stored session token from the Tauri keychain
 * 2. If a token exists, validates it with the gateway (HTTP POST /auth/validate)
 * 3. If valid, connects the persistent WebSocket
 * 4. Wires all WS frame/state/error handlers to the Zustand store
 * 5. On successful connection, auto-subscribes to default channels
 *
 * If no valid token is found the app remains in its initial
 * disconnected state and the UI shows a login prompt.
 */

import { getToken, validateToken, initGatewayUrl, getGatewayUrl } from "./session";
import { connect, onFrame, onStateChange, onError, send } from "./ws";
import { useStore } from "../state/store";
import type { ServerFrame, ErrorFrame, DegradationFrame } from "./frames";
import type { SurfaceName } from "../state/store";

// ── Wire frame handlers to Zustand store ────────────────────────────────────

/**
 * Register WS frame, state, and error callbacks that update the Zustand store.
 * Returns an unsubscribe function for cleanup.
 */
export function wireFramesToStore(): () => void {
  const unsubState = onStateChange((state) => {
    useStore.getState().setConnectionStatus(state);
  });

  const unsubFrame = onFrame((frame: ServerFrame) => {
    switch (frame.type) {
      case "degradation": {
        const d = frame as DegradationFrame;
        useStore.getState().addDegradation({
          surface: d.body.surface as SurfaceName,
          reason: d.body.reason,
          started_at: new Date().toISOString(),
        });
        break;
      }

      case "error": {
        const e = frame as ErrorFrame;
        useStore.getState().addWsError({
          code: e.body.code,
          message: e.body.message,
          trace_id: e.body.trace_id,
          details: e.body.details,
          timestamp: new Date().toISOString(),
        });
        break;
      }

      case "command_result": {
        // TODO(EP-102): Surface command results in the feed/command surface
        break;
      }

      case "pong": {
        useStore.getState().setLastPongTime(Date.now());
        break;
      }

      default: {
        // All other frame types are silently dispatched to surface-level
        // subscribers via onFrame() in EP-102+ surfaces.
        break;
      }
    }
  });

  const unsubError = onError((error: ErrorFrame) => {
    useStore.getState().addWsError({
      code: error.body.code,
      message: error.body.message,
      trace_id: error.body.trace_id,
      details: error.body.details,
      timestamp: new Date().toISOString(),
    });
  });

  return () => {
    unsubState();
    unsubFrame();
    unsubError();
  };
}

// ── Bootstrap ────────────────────────────────────────────────────────────────

/**
 * Initialize the app.
 *
 * Must be called once from app.tsx on mount. Returns a cleanup function
 * that unregisters all WS callbacks (call on unmount).
 */
export async function bootstrap(): Promise<() => void> {
  // 0. Read gateway URL from Tauri persisted config (consumes config.rs)
  await initGatewayUrl();

  // 1. Read stored token from keychain
  const token = await getToken();

  // 2. If token exists, validate with gateway
  if (token) {
    try {
      const isValid = await validateToken(token);
      if (isValid) {
        // 3. If valid, connect WS with token using the configured gateway URL
        connect(token, getGatewayUrl());
        useStore.getState().setAuthenticated(true);

        // 4. On successful connection, auto-subscribe to default channels
        //    (The subscribe frame is only sent when ws state becomes "connected",
        //     handled by the onStateChange callback wired below.)
      } else {
        useStore
          .getState()
          .setAuthError("Stored session token is no longer valid — please log in again.");
      }
    } catch {
      useStore
        .getState()
        .setAuthError("Could not validate stored session — authentication service unreachable.");
    }
  }

  // 5. Register WS frame handlers that update Zustand store
  const cleanup = wireFramesToStore();

  // Auto-subscribe to default channels once connected
  const unsubConnect = onStateChange((state) => {
    if (state === "connected") {
      send({
        type: "subscribe",
        body: { channels: ["feed", "alerts", "positions"] },
      });
    }
  });

  return () => {
    cleanup();
    unsubConnect();
  };
}
