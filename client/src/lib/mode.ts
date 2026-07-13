/**
 * Mode convenience hooks for AETHER Terminal.
 *
 * `useIsAdvanced()` returns true when mode === "advanced".
 * `useIsSimple()` returns true when mode === "simple".
 *
 * Mode is a UI presentation flag only — switching MUST NOT alter
 * data, subscriptions, permissions, or pending confirms (INV-8).
 */

import { useStore } from "../state/store";

/**
 * Returns true when the terminal is in Advanced mode.
 */
export function useIsAdvanced(): boolean {
  return useStore((s) => s.mode) === "advanced";
}

/**
 * Returns true when the terminal is in Simple mode.
 */
export function useIsSimple(): boolean {
  return useStore((s) => s.mode) === "simple";
}
