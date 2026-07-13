/**
 * Zustand store for AETHER Terminal UI state.
 *
 * Manages connection status, active surface, mode,
 * degradations, and tier. Business state lives in EP-102+ surfaces.
 */

import { create } from "zustand";

// ── Type exports ──────────────────────────────────────────────────────────────

export type SurfaceName =
  "feed" | "explain" | "simulate" | "ticket" | "command" | "alerts" | "positions" | "settings";

export type ConnectionStatus = "disconnected" | "connecting" | "connected" | "reconnecting";

export type Mode = "simple" | "advanced";

export interface Degradation {
  surface: SurfaceName;
  reason: string;
  started_at: string;
}

// ── WS error entry ────────────────────────────────────────────────────────────

export interface WsErrorEntry {
  code: string;
  message: string;
  trace_id: string;
  details?: string;
  timestamp: string;
}

// ── Store shape ───────────────────────────────────────────────────────────────

export interface AppState {
  connectionStatus: ConnectionStatus;
  activeSurface: SurfaceName;
  mode: Mode;
  degradations: Degradation[];
  tier: number | null;
  paletteOpen: boolean;
  focusMode: "mouse" | "keyboard";
  wsErrors: WsErrorEntry[];
  lastPongTime: number | null;
  authenticated: boolean;
  authError: string | null;

  // Actions
  setConnectionStatus: (status: ConnectionStatus) => void;
  setAuthenticated: (val: boolean) => void;
  setAuthError: (msg: string | null) => void;
  setActiveSurface: (surface: SurfaceName) => void;
  setMode: (mode: Mode) => void;
  toggleMode: () => void;
  setTier: (tier: number | null) => void;
  addDegradation: (degradation: Degradation) => void;
  removeDegradation: (surface: SurfaceName) => void;
  clearDegradations: () => void;
  openPalette: () => void;
  closePalette: () => void;
  setFocusMode: (mode: "mouse" | "keyboard") => void;
  addWsError: (entry: WsErrorEntry) => void;
  clearWsErrors: () => void;
  setLastPongTime: (time: number | null) => void;
}

// ── Surface labels (for display) ──────────────────────────────────────────────

export const SURFACE_LABELS: Record<SurfaceName, string> = {
  feed: "Feed",
  explain: "Explain",
  simulate: "Simulate",
  ticket: "Ticket",
  command: "Command",
  alerts: "Alerts",
  positions: "Positions",
  settings: "Settings",
};

// ── Default store ─────────────────────────────────────────────────────────────

export const useStore = create<AppState>((set) => ({
  connectionStatus: "disconnected",
  activeSurface: "feed",
  mode: "simple",
  degradations: [],
  tier: null,
  paletteOpen: false,
  focusMode: "mouse",
  wsErrors: [],
  lastPongTime: null,
  authenticated: false,
  authError: null,

  setConnectionStatus: (connectionStatus) => set({ connectionStatus }),
  setAuthenticated: (authenticated) => set({ authenticated }),
  setAuthError: (authError) => set({ authError }),
  setActiveSurface: (activeSurface) => set({ activeSurface }),
  setMode: (mode) => set({ mode }),
  toggleMode: () => set((state) => ({ mode: state.mode === "simple" ? "advanced" : "simple" })),
  setTier: (tier) => set({ tier }),
  addDegradation: (degradation) =>
    set((state) => ({
      degradations: state.degradations.some((d) => d.surface === degradation.surface)
        ? state.degradations
        : [...state.degradations, degradation],
    })),
  removeDegradation: (surface) =>
    set((state) => ({
      degradations: state.degradations.filter((d) => d.surface !== surface),
    })),
  clearDegradations: () => set({ degradations: [] }),
  openPalette: () => set({ paletteOpen: true }),
  closePalette: () => set({ paletteOpen: false }),
  setFocusMode: (focusMode) => set({ focusMode }),
  addWsError: (entry) =>
    set((state) => ({
      wsErrors: [...state.wsErrors.slice(-49), entry],
    })),
  clearWsErrors: () => set({ wsErrors: [] }),
  setLastPongTime: (lastPongTime) => set({ lastPongTime }),
}));

// Expose store on window for E2E tests (dev mode only)
if (typeof window !== "undefined" && import.meta.env.DEV) {
  (window as unknown as Record<string, unknown>).__AETHER_STORE__ = useStore;
}
