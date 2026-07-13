/**
 * ErrorState — error envelope display.
 *
 * Renders error code, message, and trace_id per ErrorEnvelope shape.
 * Shows retry button when retryable is true.
 * Never shows raw exceptions or stack traces.
 */

import { useCallback } from "react";

interface ErrorStateProps {
  code: string;
  message: string;
  trace_id: string;
  retryable?: boolean;
  onRetry?: () => void;
}

export function ErrorState({
  code,
  message,
  trace_id,
  retryable = false,
  onRetry,
}: ErrorStateProps) {
  const copyTraceId = useCallback(() => {
    navigator.clipboard.writeText(trace_id).catch(() => {
      // Clipboard write may fail in some contexts — swallow silently
    });
  }, [trace_id]);

  return (
    <div className="flex flex-col items-center justify-center px-4 py-12" role="alert">
      <div className="w-full max-w-md rounded-lg border border-red-800/30 bg-red-900/20 p-6">
        {/* Code badge */}
        <div className="mb-2 flex items-center gap-2">
          <span className="rounded bg-red-800/40 px-1.5 py-0.5 font-mono text-xs uppercase text-red-400">
            {code}
          </span>
        </div>

        {/* Message */}
        <p className="mb-4 text-sm text-gray-300">{message}</p>

        {/* Trace ID with copy */}
        <div className="flex items-center gap-2 text-xs text-gray-500">
          <span>Trace:</span>
          <code className="font-mono text-gray-400">{trace_id}</code>
          <button
            onClick={copyTraceId}
            className="rounded bg-gray-800 px-2 py-0.5 text-gray-400 transition-colors hover:bg-gray-700"
            aria-label="Copy trace ID to clipboard"
          >
            Copy
          </button>
        </div>

        {/* Retry button */}
        {retryable && onRetry && (
          <button
            onClick={onRetry}
            className="mt-4 rounded-md bg-red-800/40 px-4 py-2 text-sm font-medium text-red-400 transition-colors hover:bg-red-800/60"
          >
            Retry
          </button>
        )}
      </div>
    </div>
  );
}
