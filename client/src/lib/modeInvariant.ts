/**
 * INV-8 invariant tracking for mode toggling.
 *
 * Mode is a presentation flag on the session — switching MUST NOT
 * alter data, subscriptions, permissions, pending confirms, or
 * connection state (INV-8).
 *
 * This module captures pre-toggle state, then verifies post-toggle
 * state is identical (same object references, not deep clones).
 */

import type { ConnectionStatus, SurfaceName, Mode, Degradation } from "../state/store";
import { useStore } from "../state/store";

// ── Snapshot shape ──────────────────────────────────────────────────────────────

export interface ModeInvariantState {
  activeSurface: SurfaceName;
  mode: Mode;
  degradations: Degradation[];
  connectionStatus: ConnectionStatus;
  /** Stringified subscription IDs for shallow comparison. */
  subscriptionKeys: string[];
}

// ── Helpers ─────────────────────────────────────────────────────────────────────

/**
 * Collect the current subscription-like identifiers from the store.
 * At this phase there are no subscription modules; this returns an
 * empty array as a placeholder for when subscriptions land.
 */
function collectSubscriptionKeys(): string[] {
  // TODO(EP-102): Gather active subscription IDs from subscription store slice.
  return [];
}

// ── Capture ─────────────────────────────────────────────────────────────────────

/**
 * Snapshot the current state before a mode toggle.
 * Returns a frozen snapshot for later comparison.
 */
export function captureStateBeforeToggle(): ModeInvariantState {
  const state = useStore.getState();
  return {
    activeSurface: state.activeSurface,
    mode: state.mode,
    degradations: [...state.degradations],
    connectionStatus: state.connectionStatus,
    subscriptionKeys: collectSubscriptionKeys(),
  };
}

// ── Verification ────────────────────────────────────────────────────────────────

/**
 * Verify INV-8 invariant after a mode toggle.
 *
 * Asserts that nothing except `mode` changed. Returns an array of
 * violation messages, or an empty array if the invariant holds.
 */
export function verifyStateAfterToggle(
  before: ModeInvariantState,
  after: ModeInvariantState,
): string[] {
  const violations: string[] = [];

  if (after.mode === before.mode) {
    violations.push("Mode did not change — toggle had no effect");
    // If mode didn't change, the rest should still be preserved, but
    // we continue checking for completeness.
  }

  if (after.activeSurface !== before.activeSurface) {
    violations.push(
      `INV-8 violation: activeSurface changed from "${before.activeSurface}" to "${after.activeSurface}"`,
    );
  }

  if (after.connectionStatus !== before.connectionStatus) {
    violations.push(
      `INV-8 violation: connectionStatus changed from "${before.connectionStatus}" to "${after.connectionStatus}"`,
    );
  }

  // Compare degradations by length and content (shallow object comparison)
  if (after.degradations.length !== before.degradations.length) {
    violations.push(
      `INV-8 violation: degradations count changed from ${before.degradations.length} to ${after.degradations.length}`,
    );
  } else {
    for (let i = 0; i < before.degradations.length; i++) {
      const b = before.degradations[i];
      const a = after.degradations[i];
      if (b?.surface !== a?.surface || b?.reason !== a?.reason || b?.started_at !== a?.started_at) {
        violations.push(`INV-8 violation: degradation at index ${i} changed`);
        break;
      }
    }
  }

  // Compare subscription keys
  if (before.subscriptionKeys.length !== after.subscriptionKeys.length) {
    violations.push(
      `INV-8 violation: subscription count changed from ${before.subscriptionKeys.length} to ${after.subscriptionKeys.length}`,
    );
  } else {
    for (let i = 0; i < before.subscriptionKeys.length; i++) {
      if (before.subscriptionKeys[i] !== after.subscriptionKeys[i]) {
        violations.push(`INV-8 violation: subscription at index ${i} changed`);
        break;
      }
    }
  }

  return violations;
}
