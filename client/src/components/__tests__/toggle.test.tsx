/**
 * Tests for ModeToggle component and INV-8 mode switching behavior.
 *
 * Covers:
 * - Rendering Simple and Advanced options
 * - Click-based mode switching
 * - Always enabled regardless of connection state (mode is a UI preference)
 * - Mode persistence across surface changes
 *
 * Note: The Ctrl/Cmd+. keyboard shortcut is owned by useKeyboardRouter
 * (keyboard.ts), not by ModeToggle. Those shortcuts are tested in
 * keyboard.test.tsx and e2e/toggle.spec.ts.
 */

import { describe, it, expect, beforeEach } from "vitest";
import { render, screen, fireEvent, act } from "@testing-library/react";
import { ModeToggle } from "../toggle/ModeToggle";
import { useStore } from "../../state/store";

describe("ModeToggle", () => {
  beforeEach(() => {
    // Reset store to initial state
    useStore.setState({
      connectionStatus: "disconnected",
      activeSurface: "feed",
      mode: "simple",
      degradations: [],
      tier: null,
    });
  });

  // ── Rendering ──────────────────────────────────────────────────────────────

  it("renders Simple and Advanced options", () => {
    render(<ModeToggle />);
    expect(screen.getByText("Simple")).toBeTruthy();
    expect(screen.getByText("Advanced")).toBeTruthy();
  });

  it("shows Simple as the active option by default", () => {
    render(<ModeToggle />);
    const simple = screen.getByLabelText("Simple mode");
    const advanced = screen.getByLabelText("Advanced mode");
    // Radix Toggle Group sets aria-checked for the active item
    expect(simple.getAttribute("aria-checked")).toBe("true");
    expect(advanced.getAttribute("aria-checked")).toBe("false");
  });

  // ── Click switching ────────────────────────────────────────────────────────

  it("clicking Advanced switches mode from simple to advanced", () => {
    render(<ModeToggle />);

    const advanced = screen.getByText("Advanced");
    fireEvent.click(advanced);

    expect(useStore.getState().mode).toBe("advanced");
  });

  it("clicking Simple switches mode from advanced to simple", () => {
    useStore.setState({ mode: "advanced" });
    render(<ModeToggle />);

    const simple = screen.getByText("Simple");
    fireEvent.click(simple);

    expect(useStore.getState().mode).toBe("simple");
  });

  it("toggling mode multiple times works correctly", () => {
    render(<ModeToggle />);

    fireEvent.click(screen.getByText("Advanced"));
    expect(useStore.getState().mode).toBe("advanced");

    fireEvent.click(screen.getByText("Simple"));
    expect(useStore.getState().mode).toBe("simple");

    fireEvent.click(screen.getByText("Advanced"));
    expect(useStore.getState().mode).toBe("advanced");
  });

  // ── Always enabled (mode is a UI preference, not server-side) ──────────────

  it("toggle works even when disconnected", () => {
    useStore.setState({ connectionStatus: "disconnected" });
    render(<ModeToggle />);

    fireEvent.click(screen.getByText("Advanced"));
    expect(useStore.getState().mode).toBe("advanced");
  });

  it("toggle works when connecting", () => {
    useStore.setState({ connectionStatus: "connecting" });
    render(<ModeToggle />);

    fireEvent.click(screen.getByText("Advanced"));
    expect(useStore.getState().mode).toBe("advanced");
  });

  it("toggle works when reconnecting", () => {
    useStore.setState({ connectionStatus: "reconnecting" });
    render(<ModeToggle />);

    fireEvent.click(screen.getByText("Advanced"));
    expect(useStore.getState().mode).toBe("advanced");
  });

  // ── Mode persistence across surface changes ────────────────────────────────

  it("mode persists when switching surface", () => {
    useStore.setState({ mode: "advanced" });
    render(<ModeToggle />);

    act(() => {
      useStore.getState().setActiveSurface("explain");
    });

    expect(useStore.getState().mode).toBe("advanced");
    expect(useStore.getState().activeSurface).toBe("explain");
  });

  it("mode persists across multiple surface changes", () => {
    useStore.setState({ mode: "advanced" });
    render(<ModeToggle />);

    act(() => {
      useStore.getState().setActiveSurface("simulate");
    });
    act(() => {
      useStore.getState().setActiveSurface("settings");
    });
    act(() => {
      useStore.getState().setActiveSurface("feed");
    });

    expect(useStore.getState().mode).toBe("advanced");
  });

  it("re-renders with correct active state when mode changes externally", () => {
    const { rerender } = render(<ModeToggle />);

    act(() => {
      useStore.setState({ mode: "advanced" });
    });
    rerender(<ModeToggle />);

    const advanced = screen.getByLabelText("Advanced mode");
    expect(advanced.getAttribute("aria-checked")).toBe("true");
    expect(useStore.getState().mode).toBe("advanced");
  });

  // ── Accessibility ──────────────────────────────────────────────────────────

  it("has accessible labels for both options", () => {
    render(<ModeToggle />);
    expect(screen.getByLabelText("Simple mode")).toBeTruthy();
    expect(screen.getByLabelText("Advanced mode")).toBeTruthy();
  });

  it("has a radiogroup role for the toggle group", () => {
    render(<ModeToggle />);
    expect(screen.getByRole("radiogroup")).toBeTruthy();
  });
});
