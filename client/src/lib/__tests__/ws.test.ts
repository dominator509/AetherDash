/**
 * Tests for the WebSocket client (ws.ts).
 *
 * Covers:
 * - Frame dispatch table: all 10 server→client types are recognized
 * - Unknown frame type → error state, no crash/throw
 * - Connection state machine transitions
 * - Reconnect backoff with full jitter
 * - Auto-ping timer
 * - Client→server frame serialization for all 6 types
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

// ── Mock the WebSocket factory ───────────────────────────────────────────────

interface MockWs {
  url: string;
  readyState: number;
  onopen: null | ((e: Event) => void);
  onclose: null | ((e: CloseEvent) => void);
  onmessage: null | ((e: MessageEvent) => void);
  onerror: null | ((e: Event) => void);
  send: (d: string) => void;
  close: (c?: number, r?: string) => void;
  _open: () => void;
  _close: (c?: number, r?: string) => void;
  _error: () => void;
  _message: (d: string) => void;
  _sent: string[];
}

const mockCtx = vi.hoisted(() => {
  const _allMocks: MockWs[] = [];
  let _lastMock: MockWs | null = null;

  function createWsMock(url: string): MockWs {
    const sent: string[] = [];
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const inst: any = {
      url,
      readyState: 0,
      onopen: null,
      onclose: null,
      onmessage: null,
      onerror: null,
      send(d: string) {
        sent.push(d);
      },
      close(c?: number, r?: string) {
        inst.readyState = 3;
        inst.onclose?.(new CloseEvent("close", { code: c ?? 1000, reason: r ?? "" }));
      },
      _open() {
        inst.readyState = 1;
        inst.onopen?.(new Event("open"));
      },
      _close(c = 1000, r = "") {
        inst.readyState = 3;
        inst.onclose?.(new CloseEvent("close", { code: c, reason: r }));
      },
      _error() {
        inst.onerror?.(new Event("error"));
      },
      _message(d: string) {
        inst.onmessage?.(new MessageEvent("message", { data: d }));
      },
      _sent: sent,
    };
    _allMocks.push(inst);
    _lastMock = inst;
    return inst;
  }

  return {
    getLast: () => _lastMock,
    clear: () => {
      _allMocks.length = 0;
      _lastMock = null;
    },
    factory: { createWebSocket: (url: string) => createWsMock(url) },
  };
});

vi.mock("../websocket", () => mockCtx.factory);

// ── Now import the module under test ─────────────────────────────────────────

import {
  backoffDelay,
  connect,
  disconnect,
  send,
  onFrame,
  onStateChange,
  onError,
  getConnectionState,
  reset,
} from "../ws";
import type { ConnectionState } from "../ws";
import { SERVER_FRAME_TYPES } from "../frames";

// ── Helpers ──────────────────────────────────────────────────────────────────

function openConnection(token = "test-token"): void {
  connect(token);
  const m = mockCtx.getLast();
  if (!m) throw new Error("No mock WS created");
  m._open();
}

function receiveFrame(frame: Record<string, unknown>): void {
  const m = mockCtx.getLast();
  if (!m) throw new Error("No mock WS created");
  m._message(JSON.stringify(frame));
}

beforeEach(() => {
  reset();
  mockCtx.clear();
  vi.useFakeTimers();
});

afterEach(() => {
  reset();
  mockCtx.clear();
  vi.useRealTimers();
});

// ── Tests ────────────────────────────────────────────────────────────────────

describe("connection state machine", () => {
  it("starts disconnected", () => {
    expect(getConnectionState()).toBe("disconnected");
  });

  it("transitions to connecting on connect()", () => {
    connect("test-token");
    expect(getConnectionState()).toBe("connecting");
  });

  it("transitions to connected when socket opens", () => {
    connect("test-token");
    expect(getConnectionState()).toBe("connecting");

    const m = mockCtx.getLast();
    expect(m).not.toBeNull();
    m!._open();
    expect(getConnectionState()).toBe("connected");
  });

  it("transitions to disconnected on disconnect()", () => {
    openConnection();
    disconnect();
    expect(getConnectionState()).toBe("disconnected");
  });

  it("notifies state change callbacks", () => {
    const states: ConnectionState[] = [];
    onStateChange((s) => states.push(s));
    openConnection();
    expect(states).toContain("connected");
  });
});

describe("frame dispatch table", () => {
  it.each(SERVER_FRAME_TYPES)("dispatches server frame type: %s", (frameType) => {
    const received: string[] = [];
    onFrame((frame) => received.push(frame.type));
    openConnection();
    receiveFrame({ type: frameType, body: {} });
    expect(received).toContain(frameType);
  });

  it("dispatches frame with complex body", () => {
    const received: unknown[] = [];
    onFrame((frame) => received.push(frame));
    openConnection();
    receiveFrame({
      type: "quote",
      body: {
        market: "mkt:test:btc",
        bid: "50000",
        ask: "50001",
        ts: "2026-01-01T00:00:00.000Z",
        source: "stream",
      },
    });
    expect(received).toHaveLength(1);
    const frame = received[0] as Record<string, unknown>;
    expect(frame.type).toBe("quote");
  });

  it("dispatches to multiple callbacks", () => {
    const r1: string[] = [];
    const r2: string[] = [];
    onFrame((f) => r1.push(f.type));
    onFrame((f) => r2.push(f.type));
    openConnection();
    receiveFrame({ type: "pong", body: {} });
    expect(r1).toContain("pong");
    expect(r2).toContain("pong");
  });

  it("can unsubscribe a frame callback", () => {
    const received: string[] = [];
    const unsub = onFrame((f) => received.push(f.type));
    openConnection();
    receiveFrame({ type: "pong", body: {} });
    expect(received).toHaveLength(1);
    unsub();
    receiveFrame({ type: "pong", body: {} });
    expect(received).toHaveLength(1);
  });
});

describe("unknown frame types", () => {
  it("emits error for unknown server frame type (never crashes)", () => {
    const errors: Array<{ code: string; message: string }> = [];
    onError((err) => errors.push({ code: err.body.code, message: err.body.message }));
    openConnection();
    receiveFrame({ type: "unknown_frame_type_xyz", body: {} });
    expect(errors).toHaveLength(1);
    expect(errors[0]!.code).toBe("invalid_argument");
    expect(errors[0]!.message).toContain("unknown_frame_type_xyz");
  });

  it("does not forward unknown frames to frame callbacks", () => {
    const received: string[] = [];
    onFrame((f) => received.push(f.type));
    openConnection();
    receiveFrame({ type: "bogus_type", body: {} });
    expect(received).toHaveLength(0);
  });

  it("ignores frames with missing type field", () => {
    const errors: Array<{ code: string }> = [];
    onError((err) => errors.push({ code: err.body.code }));
    openConnection();
    receiveFrame({ body: {} });
    expect(errors).toHaveLength(0);
  });

  it("ignores non-JSON messages", () => {
    const received: string[] = [];
    onFrame((f) => received.push(f.type));
    openConnection();
    mockCtx.getLast()!._message("not valid json {{{");
    expect(received).toHaveLength(0);
  });
});

describe("reconnect backoff (SPEC-006 jitter)", () => {
  it("returns delay within [0, exponential] range", () => {
    for (let attempt = 0; attempt < 5; attempt++) {
      const delay = backoffDelay(attempt);
      const exp = Math.min(30_000, 200 * Math.pow(2, attempt));
      expect(delay).toBeGreaterThanOrEqual(0);
      expect(delay).toBeLessThanOrEqual(exp);
    }
  });

  it("returns -1 after max attempts", () => {
    expect(backoffDelay(5)).toBe(-1);
    expect(backoffDelay(10)).toBe(-1);
  });

  it("produces varied delays (jitter is random)", () => {
    const d = new Set<number>();
    for (let i = 0; i < 100; i++) d.add(backoffDelay(2));
    expect(d.size).toBeGreaterThan(1);
  });

  it("reconnect state machine transitions through reconnecting", () => {
    openConnection();
    mockCtx.getLast()!._close(1006, "Connection lost");
    expect(getConnectionState()).toBe("reconnecting");
    // Advance past reconnect delay — a new connection attempt is made
    vi.advanceTimersByTime(500);
    expect(getConnectionState()).toBe("connecting");
  });
});

describe("auto-ping", () => {
  it("sends ping frame at ~30 second intervals", () => {
    openConnection();
    const m = mockCtx.getLast()!;
    expect(m._sent).not.toContain('{"type":"ping"}');
    vi.advanceTimersByTime(30_000);
    expect(m._sent).toContain('{"type":"ping"}');
  });

  it("stops ping timer on disconnect", () => {
    openConnection();
    const m = mockCtx.getLast()!;
    const before = m._sent.length;
    disconnect();
    vi.advanceTimersByTime(30_000);
    expect(m._sent.length).toBe(before);
  });

  it("sends a ping after exactly 30 seconds of being connected", () => {
    openConnection();
    const m = mockCtx.getLast()!;
    vi.advanceTimersByTime(29_000);
    expect(m._sent).not.toContain('{"type":"ping"}');
    vi.advanceTimersByTime(1_000);
    expect(m._sent).toContain('{"type":"ping"}');
  });
});

describe("client frame serialization", () => {
  function openAndSend(frame: Record<string, unknown>): string[] {
    openConnection();
    const m = mockCtx.getLast()!;
    m._sent.length = 0;
    send(frame as unknown as Parameters<typeof send>[0]);
    return m._sent;
  }

  it("sends subscribe frame", () => {
    const sent = openAndSend({ type: "subscribe", body: { channels: ["feed"] } });
    expect(JSON.parse(sent[0]!).type).toBe("subscribe");
  });

  it("sends unsubscribe frame", () => {
    const sent = openAndSend({ type: "unsubscribe" });
    expect(JSON.parse(sent[0]!).type).toBe("unsubscribe");
  });

  it("sends command frame", () => {
    const sent = openAndSend({ type: "command", body: { text: "hi" } });
    expect(JSON.parse(sent[0]!).body.text).toBe("hi");
  });

  it("sends order_intent frame", () => {
    const sent = openAndSend({
      type: "order_intent",
      body: {
        id: "01ARZ3NDEKTSV4RRFFQ69G5FAV",
        market: "mkt:test:btc",
        side: "buy",
        order_type: "limit",
        size: "1",
        size_unit: "contracts",
        tif: "gtc",
        paper: true,
        quote_snapshot: {},
        caps_version: "01ARZ3NDEKTSV4RRFFQ69G5FAW",
        created_ts: "2026-01-01T00:00:00.000Z",
      },
    });
    expect(JSON.parse(sent[0]!).type).toBe("order_intent");
  });

  it("sends confirm frame with TOTP", () => {
    const sent = openAndSend({ type: "confirm", body: { ref_id: "r1", totp: "123456" } });
    expect(JSON.parse(sent[0]!).body.totp).toBe("123456");
  });

  it("sends ping frame", () => {
    const sent = openAndSend({ type: "ping" });
    expect(JSON.parse(sent[0]!).type).toBe("ping");
  });

  it("returns true when connected", () => {
    openConnection();
    expect(send({ type: "ping" })).toBe(true);
  });

  it("returns false when not connected", () => {
    expect(send({ type: "ping" })).toBe(false);
  });
});

describe("token handling", () => {
  it("passes token as query parameter", () => {
    connect("my-secret-token");
    expect(mockCtx.getLast()!.url).toContain("?token=my-secret-token");
  });

  it("encodes special characters in token", () => {
    connect("token with spaces&special=chars");
    expect(mockCtx.getLast()!.url).toContain("?token=token%20with%20spaces%26special%3Dchars");
  });

  it("uses default gateway URL", () => {
    connect("test-token");
    expect(mockCtx.getLast()!.url).toContain("ws://localhost:8080/ws");
  });
});

describe("edge cases", () => {
  it("does not reconnect on intentional disconnect", () => {
    openConnection();
    disconnect();
    expect(getConnectionState()).toBe("disconnected");
    vi.advanceTimersByTime(200_000);
    expect(getConnectionState()).toBe("disconnected");
  });

  it("is idempotent — calling connect twice does not create two sockets", () => {
    connect("test-token");
    const first = mockCtx.getLast();
    connect("test-token");
    expect(mockCtx.getLast()).toBe(first);
  });

  it("reset clears all state", () => {
    openConnection();
    expect(getConnectionState()).toBe("connected");
    reset();
    expect(getConnectionState()).toBe("disconnected");
  });
});
