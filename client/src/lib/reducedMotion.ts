/**
 * Reduced motion support for AETHER Terminal.
 *
 * Provides a hook and imperative helpers to detect and respond to
 * the user's `prefers-reduced-motion` system setting.
 *
 * When the preference is set to "reduce":
 * - A `reduced-motion` class is added to `<body>` for CSS targeting
 * - The `useReducedMotion()` hook returns `true`
 * - Animations should be disabled or minimized
 */

import { useState, useEffect } from "react";

// ── Media query ──────────────────────────────────────────────────────────────

const QUERY = "(prefers-reduced-motion: reduce)";

// ── Body class management ────────────────────────────────────────────────────

function updateBodyClass(reduce: boolean): void {
  if (typeof document === "undefined") return;
  document.body.classList.toggle("reduced-motion", reduce);
}

// ── React hook ───────────────────────────────────────────────────────────────

/**
 * Returns `true` when the user has requested reduced motion.
 *
 * On the initial render (SSR / hydration) conservatively returns `false`
 * and immediately re-renders if the media query matches.
 */
export function useReducedMotion(): boolean {
  const [reduce, setReduce] = useState<boolean>(() => {
    if (typeof window === "undefined") return false;
    return window.matchMedia(QUERY).matches;
  });

  useEffect(() => {
    const mql = window.matchMedia(QUERY);

    // Sync body class on mount
    updateBodyClass(mql.matches);
    setReduce(mql.matches);

    const handler = (event: MediaQueryListEvent) => {
      updateBodyClass(event.matches);
      setReduce(event.matches);
    };

    mql.addEventListener("change", handler);
    return () => {
      mql.removeEventListener("change", handler);
    };
  }, []);

  return reduce;
}

// ── Imperative one-shot ──────────────────────────────────────────────────────

/**
 * Apply the reduced-motion class to `<body>` based on current system
 * preference. Call this during application boot if the hook isn't used.
 */
export function applyReducedMotionClass(): void {
  if (typeof window === "undefined") return;
  const reduce = window.matchMedia(QUERY).matches;
  updateBodyClass(reduce);
}

/**
 * Check the current system reduced motion preference without a hook.
 */
export function prefersReducedMotion(): boolean {
  if (typeof window === "undefined") return false;
  return window.matchMedia(QUERY).matches;
}
