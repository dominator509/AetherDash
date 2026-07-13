/**
 * WsErrorOverlay — fixed-position overlay that displays WS errors.
 *
 * Reads from the wsErrors array in the Zustand store (populated by
 * bootstrap.ts frame/error wiring). Each error renders as an ErrorState
 * card. Errors can be dismissed individually or all at once.
 *
 * Unknown / malformed frames from the gateway appear here so they are
 * VISIBLE to the user, not just logged to the console.
 */

import { useCallback } from "react";
import { useStore } from "../../state/store";

export function WsErrorOverlay() {
  const wsErrors = useStore((s) => s.wsErrors);
  const clearWsErrors = useStore((s) => s.clearWsErrors);

  if (wsErrors.length === 0) {
    return null;
  }

  return (
    <div
      className="fixed bottom-4 right-4 z-50 flex max-w-sm flex-col gap-2"
      role="log"
      aria-live="assertive"
      aria-label="WebSocket errors"
    >
      {/* Clear all button */}
      {wsErrors.length > 1 && (
        <button
          onClick={clearWsErrors}
          className="self-end rounded bg-red-800/40 px-2 py-0.5 text-xs text-red-400 transition-colors hover:bg-red-800/60"
        >
          Clear all ({wsErrors.length})
        </button>
      )}

      {/* Error cards */}
      {wsErrors.map((err, i) => (
        <DismissibleError key={`${err.trace_id}-${i}`} index={i} />
      ))}
    </div>
  );
}

// ── Dismissible error item ───────────────────────────────────────────────────

function DismissibleError({ index }: { index: number }) {
  const err = useStore((s) => s.wsErrors[index]);

  const dismiss = useCallback(() => {
    // Remove this specific error by filtering it out
    useStore.setState((state) => ({
      wsErrors: state.wsErrors.filter((_, i) => i !== index),
    }));
  }, [index]);

  // If the error was already dismissed, don't render
  if (err == null) return null;

  // Use a simpler inline error display instead of the full ErrorState card
  // to keep the overlay compact
  return (
    <div className="rounded-lg border border-red-800/30 bg-red-900/20 p-3 shadow-lg" role="alert">
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0 flex-1">
          <span className="rounded bg-red-800/40 px-1.5 py-0.5 font-mono text-xs uppercase text-red-400">
            {err.code}
          </span>
          <p className="mt-1 text-xs text-gray-300">{err.message}</p>
          {err.trace_id && (
            <code className="mt-0.5 block truncate font-mono text-[10px] text-gray-500">
              Trace: {err.trace_id}
            </code>
          )}
        </div>
        <button
          onClick={dismiss}
          className="shrink-0 rounded p-0.5 text-gray-500 transition-colors hover:text-gray-300"
          aria-label="Dismiss error"
        >
          <svg className="h-3.5 w-3.5" viewBox="0 0 20 20" fill="currentColor" aria-hidden="true">
            <path
              fillRule="evenodd"
              d="M4.293 4.293a1 1 0 011.414 0L10 8.586l4.293-4.293a1 1 0 111.414 1.414L11.414 10l4.293 4.293a1 1 0 01-1.414 1.414L10 11.414l-4.293 4.293a1 1 0 01-1.414-1.414L8.586 10 4.293 5.707a1 1 0 010-1.414z"
              clipRule="evenodd"
            />
          </svg>
        </button>
      </div>
    </div>
  );
}
