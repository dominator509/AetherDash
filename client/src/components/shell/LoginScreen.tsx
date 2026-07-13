/**
 * LoginScreen — authentication gateway for AETHER Terminal.
 *
 * Renders when no valid session token exists. Collects a session token
 * string, validates it against the gateway, and on success connects
 * the persistent WebSocket. Handles loading, error, and keyboard
 * accessibility.
 */

import { useState, useCallback, type FormEvent } from "react";
import { login, getGatewayUrl } from "../../lib/session";
import { useStore } from "../../state/store";

export function LoginScreen() {
  const setAuthenticated = useStore((s) => s.setAuthenticated);
  const authError = useStore((s) => s.authError);
  const setAuthError = useStore((s) => s.setAuthError);

  const [token, setToken] = useState("");
  const [loading, setLoading] = useState(false);

  const handleSubmit = useCallback(
    async (e: FormEvent) => {
      e.preventDefault();
      if (loading) return;

      setAuthError(null);
      setLoading(true);

      try {
        await login(getGatewayUrl(), token);
        setAuthenticated(true);
      } catch (err: unknown) {
        const message =
          err instanceof Error ? err.message : "Authentication failed — an unknown error occurred.";
        setAuthError(message);
        setAuthenticated(false);
      } finally {
        setLoading(false);
      }
    },
    [token, loading, setAuthenticated, setAuthError],
  );

  return (
    <div className="flex h-screen items-center justify-center bg-gray-950">
      <div className="w-full max-w-sm rounded-lg border border-gray-800 bg-gray-900 p-8 shadow-xl">
        {/* Logo / Branding */}
        <div className="mb-8 text-center">
          <h1 className="text-2xl font-bold text-gray-100">AETHER Terminal</h1>
          <p className="mt-1 text-sm text-gray-500">Authenticate to connect</p>
        </div>

        {/* Error message */}
        {authError && (
          <div
            className="mb-6 rounded-md border border-red-800/30 bg-red-900/20 px-4 py-3"
            role="alert"
          >
            <p className="text-sm text-red-400">{authError}</p>
          </div>
        )}

        {/* Login form */}
        <form onSubmit={handleSubmit} className="flex flex-col gap-5">
          {/* Session Token */}
          <div className="flex flex-col gap-1.5">
            <label
              htmlFor="login-token"
              className="text-xs font-medium uppercase tracking-wide text-gray-400"
            >
              Session Token
            </label>
            <input
              id="login-token"
              type="password"
              autoComplete="off"
              value={token}
              onChange={(e) => setToken(e.target.value)}
              disabled={loading}
              required
              placeholder="Enter your session token"
              className="rounded-md border border-gray-700 bg-gray-800 px-3 py-2 text-sm text-gray-100 placeholder-gray-500 transition-colors focus:border-blue-600 focus:outline-none focus:ring-1 focus:ring-blue-600 disabled:opacity-50"
            />
          </div>

          {/* Connect button */}
          <button
            type="submit"
            disabled={loading}
            className="mt-2 rounded-md bg-blue-700 px-4 py-2.5 text-sm font-medium text-white transition-colors hover:bg-blue-600 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 focus:ring-offset-gray-900 disabled:cursor-not-allowed disabled:opacity-50"
          >
            {loading ? (
              <span className="flex items-center justify-center gap-2">
                <svg
                  className="h-4 w-4 animate-spin"
                  viewBox="0 0 24 24"
                  fill="none"
                  aria-hidden="true"
                >
                  <circle
                    className="opacity-25"
                    cx="12"
                    cy="12"
                    r="10"
                    stroke="currentColor"
                    strokeWidth="4"
                  />
                  <path
                    className="opacity-75"
                    fill="currentColor"
                    d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"
                  />
                </svg>
                Connecting...
              </span>
            ) : (
              "Connect"
            )}
          </button>
        </form>

        {/* Footer */}
        <p className="mt-6 text-center text-[10px] text-gray-600">
          AI is the pilot — deterministic Rust services are the engine
        </p>
      </div>
    </div>
  );
}
