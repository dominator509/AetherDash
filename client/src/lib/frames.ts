/**
 * Frame types for the SPEC-003 Gateway WebSocket protocol.
 *
 * 6 client→server types, 10 server→client types.
 * Each frame is JSON: { type, id?, trace_id?, body }.
 */

// ── Base ─────────────────────────────────────────────────────────────────────

export interface FrameBase {
  id?: string;
  trace_id?: string;
}

// ── Client→Server frame types (6) ────────────────────────────────────────────

export type ClientFrameType =
  "subscribe" | "unsubscribe" | "command" | "order_intent" | "confirm" | "ping";

export interface SubscribeFrame extends FrameBase {
  type: "subscribe";
  body: {
    channels: string[];
  };
}

export interface UnsubscribeFrame extends FrameBase {
  type: "unsubscribe";
  body?: Record<string, never>;
}

export interface CommandFrame extends FrameBase {
  type: "command";
  body: {
    text: string;
    room_context?: string;
  };
}

export interface OrderIntentFrame extends FrameBase {
  type: "order_intent";
  body: {
    id: string;
    market: string;
    side: string;
    order_type: string;
    limit_price?: string;
    size: string;
    size_unit: string;
    tif: string;
    paper: boolean;
    quote_snapshot: unknown;
    caps_version: string;
    created_ts: string;
  };
}

export interface ConfirmFrame extends FrameBase {
  type: "confirm";
  body: {
    ref_id: string;
    totp?: string;
  };
}

export interface PingFrame extends FrameBase {
  type: "ping";
  body?: Record<string, never>;
}

/** Union of all client→server frame types. */
export type ClientFrame =
  SubscribeFrame | UnsubscribeFrame | CommandFrame | OrderIntentFrame | ConfirmFrame | PingFrame;

// ── Server→Client frame types (10) ───────────────────────────────────────────

export type ServerFrameType =
  | "feed_item"
  | "quote"
  | "order_update"
  | "alert"
  | "explain"
  | "command_result"
  | "confirm_required"
  | "degradation"
  | "error"
  | "pong";

export interface FeedItemFrame extends FrameBase {
  type: "feed_item";
  body: Record<string, unknown>;
}

export interface QuoteFrame extends FrameBase {
  type: "quote";
  body: Record<string, unknown>;
}

export interface OrderUpdateFrame extends FrameBase {
  type: "order_update";
  body: Record<string, unknown>;
}

export interface AlertFrame extends FrameBase {
  type: "alert";
  body: Record<string, unknown>;
}

export interface ExplainFrame extends FrameBase {
  type: "explain";
  body: Record<string, unknown>;
}

export interface CommandResultFrame extends FrameBase {
  type: "command_result";
  body?: Record<string, unknown>;
}

export interface ConfirmRequiredFrame extends FrameBase {
  type: "confirm_required";
  body: {
    ref_id: string;
    action_summary: string;
    tier_reason: string;
  };
}

export interface DegradationFrame extends FrameBase {
  type: "degradation";
  body: {
    surface: string;
    reason: string;
  };
}

export interface ErrorFrame extends FrameBase {
  type: "error";
  body: {
    code: string;
    message: string;
    retryable: boolean;
    trace_id: string;
    details?: string;
  };
}

export interface PongFrame extends FrameBase {
  type: "pong";
  body?: Record<string, never>;
}

/** Union of all server→client frame types. */
export type ServerFrame =
  | FeedItemFrame
  | QuoteFrame
  | OrderUpdateFrame
  | AlertFrame
  | ExplainFrame
  | CommandResultFrame
  | ConfirmRequiredFrame
  | DegradationFrame
  | ErrorFrame
  | PongFrame;

/** Union of all frame types (both directions). */
export type AnyFrame = ClientFrame | ServerFrame;

// ── Dispatch table helper ────────────────────────────────────────────────────

/** Map from server frame type to its TypeScript interface constructor (nominal). */
export const SERVER_FRAME_TYPES: readonly ServerFrameType[] = [
  "feed_item",
  "quote",
  "order_update",
  "alert",
  "explain",
  "command_result",
  "confirm_required",
  "degradation",
  "error",
  "pong",
] as const;

/** Map from client frame type to its TypeScript interface constructor (nominal). */
export const CLIENT_FRAME_TYPES: readonly ClientFrameType[] = [
  "subscribe",
  "unsubscribe",
  "command",
  "order_intent",
  "confirm",
  "ping",
] as const;
