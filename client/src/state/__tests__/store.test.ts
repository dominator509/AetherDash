/**
 * Tests for Zustand store state transitions.
 */

import { describe, it, expect, beforeEach } from "vitest";
import { useStore } from "../store";

describe("store", () => {
  beforeEach(() => {
    // Reset store to initial state between tests
    useStore.setState({
      connectionStatus: "disconnected",
      activeSurface: "feed",
      mode: "simple",
      degradations: [],
      tier: null,
    });
  });

  // ── Connection status transitions ──────────────────────────────────────────

  it("starts as disconnected", () => {
    expect(useStore.getState().connectionStatus).toBe("disconnected");
  });

  it("transitions to connecting", () => {
    useStore.getState().setConnectionStatus("connecting");
    expect(useStore.getState().connectionStatus).toBe("connecting");
  });

  it("transitions from connecting to connected", () => {
    useStore.getState().setConnectionStatus("connecting");
    useStore.getState().setConnectionStatus("connected");
    expect(useStore.getState().connectionStatus).toBe("connected");
  });

  it("transitions from connected to reconnecting", () => {
    useStore.getState().setConnectionStatus("connected");
    useStore.getState().setConnectionStatus("reconnecting");
    expect(useStore.getState().connectionStatus).toBe("reconnecting");
  });

  it("transitions from reconnecting to disconnected", () => {
    useStore.getState().setConnectionStatus("reconnecting");
    useStore.getState().setConnectionStatus("disconnected");
    expect(useStore.getState().connectionStatus).toBe("disconnected");
  });

  // ── Active surface switching ───────────────────────────────────────────────

  it("starts with feed as active surface", () => {
    expect(useStore.getState().activeSurface).toBe("feed");
  });

  it("switches to explain surface", () => {
    useStore.getState().setActiveSurface("explain");
    expect(useStore.getState().activeSurface).toBe("explain");
  });

  it("switches to simulate surface", () => {
    useStore.getState().setActiveSurface("simulate");
    expect(useStore.getState().activeSurface).toBe("simulate");
  });

  it("switches to settings surface", () => {
    useStore.getState().setActiveSurface("settings");
    expect(useStore.getState().activeSurface).toBe("settings");
  });

  it("switches back to feed", () => {
    useStore.getState().setActiveSurface("settings");
    useStore.getState().setActiveSurface("feed");
    expect(useStore.getState().activeSurface).toBe("feed");
  });

  // ── Mode toggling ─────────────────────────────────────────────────────────

  it("starts in simple mode", () => {
    expect(useStore.getState().mode).toBe("simple");
  });

  it("setMode('advanced') updates mode", () => {
    useStore.getState().setMode("advanced");
    expect(useStore.getState().mode).toBe("advanced");
  });

  it("setMode('simple') from advanced works", () => {
    useStore.getState().setMode("advanced");
    useStore.getState().setMode("simple");
    expect(useStore.getState().mode).toBe("simple");
  });

  it("toggleMode switches from simple to advanced", () => {
    useStore.getState().toggleMode();
    expect(useStore.getState().mode).toBe("advanced");
  });

  it("toggleMode switches from advanced to simple", () => {
    useStore.getState().setMode("advanced");
    useStore.getState().toggleMode();
    expect(useStore.getState().mode).toBe("simple");
  });

  it("toggleMode works multiple times", () => {
    useStore.getState().toggleMode(); // simple -> advanced
    useStore.getState().toggleMode(); // advanced -> simple
    useStore.getState().toggleMode(); // simple -> advanced
    expect(useStore.getState().mode).toBe("advanced");
  });

  it("mode toggling does NOT affect connection status (INV-8)", () => {
    useStore.getState().setConnectionStatus("connected");
    useStore.getState().toggleMode();
    expect(useStore.getState().mode).toBe("advanced");
    expect(useStore.getState().connectionStatus).toBe("connected");
  });

  it("mode toggling does NOT affect active surface (INV-8)", () => {
    useStore.getState().setActiveSurface("settings");
    useStore.getState().toggleMode();
    expect(useStore.getState().activeSurface).toBe("settings");
    expect(useStore.getState().mode).toBe("advanced");
  });

  it("mode toggling does NOT modify degradation list (INV-8)", () => {
    useStore.getState().addDegradation({
      surface: "feed",
      reason: "Test",
      started_at: "2026-07-11T12:00:00.000Z",
    });
    useStore.getState().toggleMode();
    expect(useStore.getState().degradations).toHaveLength(1);
    expect(useStore.getState().degradations[0]?.reason).toBe("Test");
  });

  // ── Degradation list management ────────────────────────────────────────────

  it("starts with empty degradations", () => {
    expect(useStore.getState().degradations).toEqual([]);
  });

  it("adds a degradation", () => {
    useStore.getState().addDegradation({
      surface: "feed",
      reason: "Upstream latency",
      started_at: "2026-07-11T12:00:00.000Z",
    });
    expect(useStore.getState().degradations).toHaveLength(1);
    expect(useStore.getState().degradations[0]?.surface).toBe("feed");
  });

  it("does not duplicate a degradation for the same surface", () => {
    useStore.getState().addDegradation({
      surface: "feed",
      reason: "Upstream latency",
      started_at: "2026-07-11T12:00:00.000Z",
    });
    useStore.getState().addDegradation({
      surface: "feed",
      reason: "Upstream latency",
      started_at: "2026-07-11T12:00:00.000Z",
    });
    expect(useStore.getState().degradations).toHaveLength(1);
  });

  it("adds degradations for different surfaces", () => {
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
    expect(useStore.getState().degradations).toHaveLength(2);
  });

  it("removes a degradation by surface", () => {
    useStore.getState().addDegradation({
      surface: "feed",
      reason: "Upstream latency",
      started_at: "2026-07-11T12:00:00.000Z",
    });
    useStore.getState().addDegradation({
      surface: "explain",
      reason: "Explain degraded",
      started_at: "2026-07-11T12:00:00.000Z",
    });
    useStore.getState().removeDegradation("feed");
    expect(useStore.getState().degradations).toHaveLength(1);
    expect(useStore.getState().degradations[0]?.surface).toBe("explain");
  });

  it("clears all degradations", () => {
    useStore.getState().addDegradation({
      surface: "feed",
      reason: "Latency",
      started_at: "2026-07-11T12:00:00.000Z",
    });
    useStore.getState().clearDegradations();
    expect(useStore.getState().degradations).toEqual([]);
  });

  // ── Tier management ────────────────────────────────────────────────────────

  it("starts with null tier", () => {
    expect(useStore.getState().tier).toBeNull();
  });

  it("sets tier to a number", () => {
    useStore.getState().setTier(3);
    expect(useStore.getState().tier).toBe(3);
  });

  it("clears tier with null", () => {
    useStore.getState().setTier(3);
    useStore.getState().setTier(null);
    expect(useStore.getState().tier).toBeNull();
  });
});
