/**
 * Tests for INV-8 invariant tracking (modeInvariant module).
 *
 * Verifies that mode switching does NOT alter:
 * - active surface
 * - connection status
 * - pending confirms (degradations as proxy)
 * - subscriptions
 * - data identity (same references preserved)
 */

import { describe, it, expect, beforeEach } from "vitest";
import { useStore } from "../../state/store";
import { captureStateBeforeToggle, verifyStateAfterToggle } from "../modeInvariant";

describe("modeInvariant — INV-8", () => {
  beforeEach(() => {
    // Reset store to a known state
    useStore.setState({
      connectionStatus: "connected",
      activeSurface: "feed",
      mode: "simple",
      degradations: [],
      tier: null,
    });
  });

  // ── Basic capture / verify ──────────────────────────────────────────────────

  it("captures current state snapshot", () => {
    const snapshot = captureStateBeforeToggle();
    expect(snapshot.mode).toBe("simple");
    expect(snapshot.activeSurface).toBe("feed");
    expect(snapshot.connectionStatus).toBe("connected");
    expect(snapshot.degradations).toEqual([]);
    expect(snapshot.subscriptionKeys).toEqual([]);
  });

  it("verifyStateAfterToggle returns empty array when only mode changed", () => {
    const before = captureStateBeforeToggle();
    useStore.getState().setMode("advanced");
    const after = captureStateBeforeToggle();

    const violations = verifyStateAfterToggle(before, after);
    expect(violations).toEqual([]);
  });

  it("verifyStateAfterToggle returns empty array when toggling back", () => {
    useStore.getState().setMode("advanced");
    const before = captureStateBeforeToggle();
    useStore.getState().setMode("simple");
    const after = captureStateBeforeToggle();

    const violations = verifyStateAfterToggle(before, after);
    expect(violations).toEqual([]);
  });

  // ── INV-8: active surface unchanged ─────────────────────────────────────────

  it("INV-8: active surface unchanged after toggle", () => {
    useStore.getState().setActiveSurface("settings");
    const before = captureStateBeforeToggle();

    useStore.getState().setMode("advanced");
    const after = captureStateBeforeToggle();

    const violations = verifyStateAfterToggle(before, after);
    expect(violations).toEqual([]);
    expect(after.activeSurface).toBe("settings");
  });

  it("INV-8: detects active surface change as violation", () => {
    const before = captureStateBeforeToggle();
    // Simulate a toggle that also (wrongly) changes activeSurface
    const badAfter = { ...before, mode: "advanced" as const, activeSurface: "explain" as const };

    const violations = verifyStateAfterToggle(before, badAfter);
    expect(violations.some((v) => v.includes("activeSurface"))).toBe(true);
  });

  // ── INV-8: connection status unchanged ──────────────────────────────────────

  it("INV-8: connection status unchanged after toggle", () => {
    const before = captureStateBeforeToggle();
    useStore.getState().setMode("advanced");
    const after = captureStateBeforeToggle();

    const violations = verifyStateAfterToggle(before, after);
    expect(violations).toEqual([]);
    expect(after.connectionStatus).toBe("connected");
  });

  it("INV-8: detects connection status change as violation", () => {
    const before = captureStateBeforeToggle();
    const badAfter = {
      ...before,
      mode: "advanced" as const,
      connectionStatus: "disconnected" as const,
    };

    const violations = verifyStateAfterToggle(before, badAfter);
    expect(violations.some((v) => v.includes("connectionStatus"))).toBe(true);
  });

  // ── INV-8: pending confirms (degradations as proxy) unchanged ──────────────

  it("INV-8: pending confirms unchanged after toggle", () => {
    useStore.getState().addDegradation({
      surface: "feed",
      reason: "Upstream latency",
      started_at: "2026-07-11T12:00:00.000Z",
    });
    const before = captureStateBeforeToggle();

    useStore.getState().setMode("advanced");
    const after = captureStateBeforeToggle();

    const violations = verifyStateAfterToggle(before, after);
    expect(violations).toEqual([]);
    expect(after.degradations).toHaveLength(1);
    expect(after.degradations[0]?.surface).toBe("feed");
  });

  it("INV-8: detects degradation list change as violation", () => {
    const before = captureStateBeforeToggle();
    const badAfter = {
      ...before,
      mode: "advanced" as const,
      degradations: [
        { surface: "feed" as const, reason: "test", started_at: "2026-01-01T00:00:00.000Z" },
      ],
    };

    const violations = verifyStateAfterToggle(before, badAfter);
    expect(violations.some((v) => v.includes("degradation"))).toBe(true);
  });

  it("INV-8: detects degradation content change as violation", () => {
    useStore.getState().addDegradation({
      surface: "feed",
      reason: "Original reason",
      started_at: "2026-07-11T12:00:00.000Z",
    });
    const before = captureStateBeforeToggle();

    // Modify a degradation (mode switching should never do this)
    const badState = useStore.getState();
    useStore.setState({
      ...badState,
      mode: "advanced",
      degradations: [
        {
          surface: "feed" as const,
          reason: "Changed reason",
          started_at: "2026-07-11T12:00:00.000Z",
        },
      ],
    });
    const after = captureStateBeforeToggle();

    const violations = verifyStateAfterToggle(before, after);
    expect(violations.some((v) => v.includes("degradation"))).toBe(true);
  });

  // ── INV-8: subscriptions unchanged ─────────────────────────────────────────

  it("INV-8: subscriptions unchanged after toggle", () => {
    const before = captureStateBeforeToggle();
    useStore.getState().setMode("advanced");
    const after = captureStateBeforeToggle();

    const violations = verifyStateAfterToggle(before, after);
    // At this phase subscriptionKeys is always empty, so no violations
    expect(violations).toEqual([]);
  });

  it("INV-8: detects subscription change as violation", () => {
    const before = captureStateBeforeToggle();
    const badAfter = { ...before, mode: "advanced" as const, subscriptionKeys: ["sub1"] };

    const violations = verifyStateAfterToggle(before, badAfter);
    expect(violations.some((v) => v.includes("subscription"))).toBe(true);
  });

  // ── INV-8: mode must change ────────────────────────────────────────────────

  it("INV-8: reports when mode did not change", () => {
    const before = captureStateBeforeToggle();
    const sameAfter = { ...before };

    const violations = verifyStateAfterToggle(before, sameAfter);
    expect(violations.length).toBeGreaterThan(0);
    expect(violations[0]).toContain("Mode did not change");
  });

  // ── Multiple degradations ──────────────────────────────────────────────────

  it("preserves multiple degradations through toggle", () => {
    useStore.getState().addDegradation({
      surface: "feed",
      reason: "Feed latency",
      started_at: "2026-07-11T12:00:00.000Z",
    });
    useStore.getState().addDegradation({
      surface: "explain",
      reason: "Explain degraded",
      started_at: "2026-07-11T12:00:00.000Z",
    });

    const before = captureStateBeforeToggle();
    useStore.getState().setMode("advanced");
    const after = captureStateBeforeToggle();

    const violations = verifyStateAfterToggle(before, after);
    expect(violations).toEqual([]);
    expect(after.degradations).toHaveLength(2);
  });

  // ── Edge cases ─────────────────────────────────────────────────────────────

  it("handles empty degradation list", () => {
    const before = captureStateBeforeToggle();
    useStore.getState().setMode("advanced");
    const after = captureStateBeforeToggle();

    expect(before.degradations).toEqual([]);
    expect(after.degradations).toEqual([]);
    expect(verifyStateAfterToggle(before, after)).toEqual([]);
  });

  it("mode can toggle multiple times without INV-8 violation", () => {
    // Toggle simple -> advanced -> simple -> advanced
    useStore.getState().setMode("advanced");
    useStore.getState().setMode("simple");
    useStore.getState().setMode("advanced");
    expect(useStore.getState().mode).toBe("advanced");

    // Verify no violations in any direction
    const before = captureStateBeforeToggle();
    useStore.getState().setMode("simple");
    const after = captureStateBeforeToggle();

    expect(verifyStateAfterToggle(before, after)).toEqual([]);
  });
});
