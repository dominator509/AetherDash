/**
 * Keyboard router and feed keyboard hook for AETHER Terminal.
 *
 * useKeyboardRouter — mounts at AppFrame level, handles global shortcuts
 * and layer back-stack (SPEC-004 keymap).
 * useFeedKeyboard — stub for EP-102 feed keyboard interaction.
 */

import { useEffect, useRef } from "react";
import { useStore } from "../state/store";

// ── Layer type for the Escape back stack ──────────────────────────────────────

/**
 * Represents an open layer in the navigation stack.
 * Escape pops the top layer; each layer type defines what "close" means.
 */
export type LayerType = "palette" | "modal" | "panel";

export interface Layer {
  type: LayerType;
}

// ── Keyboard router ───────────────────────────────────────────────────────────

/**
 * Global keyboard router implementing the SPEC-004 keymap.
 *
 * | Key           | Context     | Action                       |
 * |---------------|-------------|------------------------------|
 * | Ctrl/Cmd+K    | Anywhere    | Open command palette         |
 * | j             | Feed/list   | Move down (EP-102)           |
 * | k             | Feed/list   | Move up (EP-102)             |
 * | e             | Item sel.   | Explain (EP-102)             |
 * | s             | Item sel.   | Simulate (EP-102)            |
 * | a             | Item sel.   | Act (EP-102)                 |
 * | i             | Item sel.   | Ignore (EP-102)              |
 * | Enter         | Item sel.   | Open detail (EP-102)         |
 * | Escape        | Anywhere    | Back out one layer            |
 * | Ctrl/Cmd+.    | Anywhere    | Toggle Simple/Advanced mode  |
 */
export function useKeyboardRouter(): void {
  const layerStack = useRef<Layer[]>([]);

  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      const state = useStore.getState();

      // ── Ctrl/Cmd+K: always opens palette (even when input is focused) ──
      if ((e.ctrlKey || e.metaKey) && e.key === "k") {
        e.preventDefault();
        e.stopPropagation();
        state.openPalette();
        layerStack.current.push({ type: "palette" });
        return;
      }

      // ── Ctrl/Cmd+.: toggle mode (only when connected) ──
      if ((e.ctrlKey || e.metaKey) && e.key === ".") {
        e.preventDefault();
        if (state.connectionStatus === "connected") {
          state.toggleMode();
        }
        return;
      }

      // Do not intercept when typing in input/textarea/contenteditable
      const target = e.target as HTMLElement;
      const isInput =
        target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.isContentEditable;

      if (isInput) {
        return;
      }

      // ── Escape: back out one layer ──
      if (e.key === "Escape") {
        // If palette is open, close it (pop from stack)
        if (state.paletteOpen) {
          e.preventDefault();
          state.closePalette();
          // Pop palette from stack if present
          for (let i = layerStack.current.length - 1; i >= 0; i--) {
            if (layerStack.current[i]?.type === "palette") {
              layerStack.current.splice(i, 1);
              break;
            }
          }
          return;
        }

        // Otherwise, pop from layer stack for non-palette layers
        if (layerStack.current.length > 0) {
          e.preventDefault();
          const layer = layerStack.current.pop();
          if (layer?.type === "modal") {
            // Future: close modal
          } else if (layer?.type === "panel") {
            // Future: close panel
          }
        }
        return;
      }

      // ── j/k/e/s/a/i/Enter: delegate to active surface (EP-102+) ──
      // For now the keys are consumed without action (stub).
      if (["j", "k", "e", "s", "a", "i", "Enter"].includes(e.key)) {
        e.preventDefault();
        return;
      }
    }

    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, []);
}

// ── Feed keyboard stub (EP-102) ────────────────────────────────────────────────

export interface FeedKeyboardState {
  selectedIndex: number;
}

/**
 * Stub — real feed keyboard interaction comes in EP-102.
 *
 * Returns a stable selectedIndex placeholder.
 * Future: handles j/k/e/s/a/i/Enter within the feed surface.
 */
export function useFeedKeyboard(): FeedKeyboardState {
  return { selectedIndex: -1 };
}
