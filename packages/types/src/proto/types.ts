// Proto mirror: aether/core/v1/types.proto — Canonical scalar types
// D7: hand-mirrored, matches proto definitions exactly

// ── Enums ──
export type ErrorCode =
  | "unspecified"
  | "invalid_argument"
  | "unauthenticated"
  | "permission_denied"
  | "not_found"
  | "failed_precondition"
  | "unavailable"
  | "deadline_exceeded"
  | "quarantined"
  | "internal";

// ── Proto wrapper messages ──
export interface Ulid {
  value: string;
}

export interface MarketKey {
  value: string;
}

export interface VenueId {
  value: string;
}

export interface Money {
  amount: string;
  currency: string;
}

export interface UtcTime {
  ts: string;
}

export interface Confidence {
  value: string;
}

export interface AuditEvent {
  seq: number;
  prev_hash: string;
  hash: string;
  ts: string;
  actor: string;
  action: string;
  subject: string;
  payload_hash: string;
}

export interface ErrorEnvelope {
  code: ErrorCode;
  message: string;
  retryable: boolean;
  trace_id: Ulid;
  details?: string;
}
