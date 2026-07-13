/**
 * Thin wrapper around the global WebSocket constructor.
 *
 * This module exists so vitest can mock it via `vi.mock()` without
 * needing to touch the global scope. The mock injects a controllable
 * WebSocket for tests.
 */

export interface WebSocketLike {
  readyState: number;
  onopen: ((event: Event) => void) | null;
  onclose: ((event: CloseEvent) => void) | null;
  onmessage: ((event: MessageEvent) => void) | null;
  onerror: ((event: Event) => void) | null;
  send: (data: string) => void;
  close: (code?: number, reason?: string) => void;
}

export function createWebSocket(url: string): WebSocketLike {
  return new WebSocket(url);
}
