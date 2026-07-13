/**
 * Test setup file — provides a mock WebSocket implementation for jsdom.
 *
 * Sets up a global WebSocket constructor that creates controllable mock
 * instances so tests can simulate open/close/message events.
 */

import { afterEach } from "vitest";

// ── Fix Radix act() warnings ───────────────────────────────────────────────────
// Radix uses requestAnimationFrame for Presence animations. In jsdom, there is
// no native rAF, so Vitest provides a stub. We make it fire synchronously so
// that animation-frame callbacks land inside React's act() scope rather than
// bleeding out and triggering "not wrapped in act()" warnings.
globalThis.requestAnimationFrame = (cb: FrameRequestCallback): number => {
  cb(0);
  return 0;
};

// ── Mock WebSocket type ──────────────────────────────────────────────────────

export interface MockWebSocket {
  url: string;
  readyState: number;
  onopen: ((event: Event) => void) | null;
  onclose: ((event: CloseEvent) => void) | null;
  onmessage: ((event: MessageEvent) => void) | null;
  onerror: ((event: Event) => void) | null;
  send: (data: string) => void;
  close: (code?: number, reason?: string) => void;
  _open: () => void;
  _close: (code?: number, reason?: string) => void;
  _error: () => void;
  _message: (data: string) => void;
  _sent: string[];
}

// ── Instance tracking ────────────────────────────────────────────────────────

const allInstances: MockWebSocket[] = [];
let lastInstance: MockWebSocket | null = null;

function makeMock(url: string | URL): MockWebSocket {
  const sent: string[] = [];

  const instance: MockWebSocket = {
    url: typeof url === "string" ? url : url.toString(),
    readyState: 0,
    onopen: null,
    onclose: null,
    onmessage: null,
    onerror: null,
    send(data: string) {
      sent.push(data);
    },
    close(code?: number, reason?: string) {
      instance.readyState = 3;
      if (instance.onclose) {
        instance.onclose(new CloseEvent("close", { code: code ?? 1000, reason: reason ?? "" }));
      }
    },
    _open() {
      instance.readyState = 1;
      instance.onopen?.(new Event("open"));
    },
    _close(code = 1000, reason = "") {
      instance.readyState = 3;
      instance.onclose?.(new CloseEvent("close", { code, reason }));
    },
    _error() {
      instance.onerror?.(new Event("error"));
    },
    _message(data: string) {
      instance.onmessage?.(new MessageEvent("message", { data }));
    },
    _sent: sent,
  };

  allInstances.push(instance);
  lastInstance = instance;
  return instance;
}

// Install the mock constructor globally
// Using assignment rather than defineProperty to ensure broad compatibility
(globalThis as Record<string, unknown>).WebSocket = Object.assign(
  (url: string | URL) => makeMock(url),
  { CONNECTING: 0, OPEN: 1, CLOSING: 2, CLOSED: 3 },
);

// ── Exported helpers ─────────────────────────────────────────────────────────

export function getLastMockWs(): MockWebSocket | null {
  return lastInstance;
}

export function clearMockWs(): void {
  allInstances.length = 0;
  lastInstance = null;
}

afterEach(() => {
  clearMockWs();
});
