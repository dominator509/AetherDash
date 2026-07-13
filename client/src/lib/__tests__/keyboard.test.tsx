/**
 * Tests for keyboard router, feed keyboard hook, and focus management.
 *
 * Covers:
 * - useKeyboardRouter: Ctrl+K, Ctrl+., Esc back-stack, surface delegation
 * - useFeedKeyboard: stub returns correct shape
 * - useFocusTrap: Tab cycling, focus restore
 */

import { describe, it, expect, beforeEach } from "vitest";
import { render, fireEvent, screen } from "@testing-library/react";
import { useRef } from "react";
import { useStore } from "../../state/store";
import { useKeyboardRouter, useFeedKeyboard } from "../keyboard";
import { useFocusTrap } from "../focus";

// ── Test harnesses ────────────────────────────────────────────────────────

function RouterHarness() {
  useKeyboardRouter();
  return (
    <div>
      <input data-testid="input-el" />
      <button data-testid="btn">Button</button>
    </div>
  );
}

function FeedHarness() {
  const state = useFeedKeyboard();
  return <div data-testid="feed-state">{JSON.stringify(state)}</div>;
}

function TrapHarness() {
  const ref = useRef<HTMLDivElement>(null);
  useFocusTrap(ref);
  return (
    <div ref={ref} data-testid="trap-container">
      <button data-testid="btn1">First</button>
      <button data-testid="btn2">Second</button>
      <button data-testid="btn3">Third</button>
    </div>
  );
}

// ── Setup ─────────────────────────────────────────────────────────────────

beforeEach(() => {
  // Reset palette state
  useStore.setState({ paletteOpen: false, focusMode: "mouse" });
});

// ── useKeyboardRouter ─────────────────────────────────────────────────────

describe("useKeyboardRouter", () => {
  it("Ctrl+K dispatches palette-open action", () => {
    render(<RouterHarness />);
    expect(useStore.getState().paletteOpen).toBe(false);

    fireEvent.keyDown(document, { key: "k", ctrlKey: true });
    expect(useStore.getState().paletteOpen).toBe(true);
  });

  it("Meta+K dispatches palette-open action", () => {
    render(<RouterHarness />);
    fireEvent.keyDown(document, { key: "k", metaKey: true });
    expect(useStore.getState().paletteOpen).toBe(true);
  });

  it("Ctrl+K works even when input is focused", () => {
    render(<RouterHarness />);
    const input = screen.getByTestId("input-el");
    input.focus();

    fireEvent.keyDown(input, { key: "k", ctrlKey: true });
    expect(useStore.getState().paletteOpen).toBe(true);
  });

  it("does not intercept other keys when input is focused", () => {
    render(<RouterHarness />);
    const input = screen.getByTestId("input-el");
    input.focus();

    // Pressing 'j' in input should not affect palette
    fireEvent.keyDown(input, { key: "j" });
    expect(useStore.getState().paletteOpen).toBe(false);
  });

  it("does not crash or close anything on Escape with palette closed", () => {
    render(<RouterHarness />);
    expect(() => {
      fireEvent.keyDown(document, { key: "Escape" });
    }).not.toThrow();
    expect(useStore.getState().paletteOpen).toBe(false);
  });

  it("Escape closes palette via back-stack when palette is open", () => {
    render(<RouterHarness />);
    // Open palette AFTER the subscription effect has mounted
    useStore.getState().openPalette();
    expect(useStore.getState().paletteOpen).toBe(true);

    // The subscribe listener should have pushed palette to the stack
    // Escape should now close it
    fireEvent.keyDown(document, { key: "Escape" });
    expect(useStore.getState().paletteOpen).toBe(false);
  });

  // ── Ctrl/Cmd+. toggle mode ─────────────────────────────────────────────

  it("Ctrl+. toggles mode from simple to advanced", () => {
    useStore.setState({ connectionStatus: "connected", mode: "simple" });
    render(<RouterHarness />);

    fireEvent.keyDown(document, { key: ".", ctrlKey: true });
    expect(useStore.getState().mode).toBe("advanced");
  });

  it("Meta+. toggles mode from advanced to simple", () => {
    useStore.setState({ connectionStatus: "connected", mode: "advanced" });
    render(<RouterHarness />);

    fireEvent.keyDown(document, { key: ".", metaKey: true });
    expect(useStore.getState().mode).toBe("simple");
  });

  it("period without Ctrl/Cmd does not toggle mode", () => {
    useStore.setState({ connectionStatus: "connected", mode: "simple" });
    render(<RouterHarness />);

    fireEvent.keyDown(document, { key: "." });
    expect(useStore.getState().mode).toBe("simple");
  });

  it("Ctrl+. does not toggle mode when disconnected", () => {
    useStore.setState({ connectionStatus: "disconnected", mode: "simple" });
    render(<RouterHarness />);

    fireEvent.keyDown(document, { key: ".", ctrlKey: true });
    expect(useStore.getState().mode).toBe("simple");
  });

  it("Ctrl+. does not toggle mode when connecting", () => {
    useStore.setState({ connectionStatus: "connecting", mode: "simple" });
    render(<RouterHarness />);

    fireEvent.keyDown(document, { key: ".", ctrlKey: true });
    expect(useStore.getState().mode).toBe("simple");
  });

  it("Ctrl+. does not toggle mode when reconnecting", () => {
    useStore.setState({ connectionStatus: "reconnecting", mode: "simple" });
    render(<RouterHarness />);

    fireEvent.keyDown(document, { key: ".", ctrlKey: true });
    expect(useStore.getState().mode).toBe("simple");
  });

  // ── Surface delegation (j/k/e/s/a/i/Enter) ─────────────────────────────

  it("j key does not crash when no input is focused", () => {
    render(<RouterHarness />);
    expect(() => {
      fireEvent.keyDown(document, { key: "j" });
    }).not.toThrow();
  });

  it("k key does not crash when no input is focused", () => {
    render(<RouterHarness />);
    expect(() => {
      fireEvent.keyDown(document, { key: "k" });
    }).not.toThrow();
  });

  it("Enter key does not crash when no input is focused", () => {
    render(<RouterHarness />);
    expect(() => {
      fireEvent.keyDown(document, { key: "Enter" });
    }).not.toThrow();
  });
});

// ── useFeedKeyboard ───────────────────────────────────────────────────────

describe("useFeedKeyboard (stub)", () => {
  it("returns a stub with selectedIndex of -1", () => {
    render(<FeedHarness />);
    const el = screen.getByTestId("feed-state");
    const state = JSON.parse(el.textContent ?? "{}");
    expect(state).toEqual({ selectedIndex: -1 });
  });

  it("returns a stable reference", () => {
    const { rerender } = render(<FeedHarness />);
    const el1 = screen.getByTestId("feed-state");
    const val1 = el1.textContent;

    // Trigger re-render
    useStore.getState().openPalette();
    useStore.getState().closePalette();

    rerender(<FeedHarness />);
    const el2 = screen.getByTestId("feed-state");
    expect(el2.textContent).toBe(val1);
  });
});

// ── useFocusTrap ──────────────────────────────────────────────────────────

describe("useFocusTrap", () => {
  it("focuses the first focusable element on mount", () => {
    render(<TrapHarness />);
    const btn1 = screen.getByTestId("btn1");
    // The first button should be focused by the trap
    expect(document.activeElement).toBe(btn1);
  });

  it("traps Tab focus within the container cycling forward", () => {
    render(<TrapHarness />);
    const btn1 = screen.getByTestId("btn1");
    const btn2 = screen.getByTestId("btn2");
    const btn3 = screen.getByTestId("btn3");

    // Tab from btn1 — normally would go to btn2
    fireEvent.keyDown(btn1, { key: "Tab" });
    btn2.focus();
    expect(document.activeElement).toBe(btn2);

    // Tab from btn2 — normally would go to btn3
    fireEvent.keyDown(btn2, { key: "Tab" });
    btn3.focus();
    expect(document.activeElement).toBe(btn3);

    // Tab from btn3 — focus trap wraps to btn1
    fireEvent.keyDown(btn3, { key: "Tab" });
    expect(document.activeElement).toBe(btn1);
  });

  it("traps Shift+Tab focus cycling backward", () => {
    render(<TrapHarness />);
    const btn1 = screen.getByTestId("btn1");
    const btn2 = screen.getByTestId("btn2");
    const btn3 = screen.getByTestId("btn3");

    // Shift+Tab on first wraps to last
    fireEvent.keyDown(btn1, { key: "Tab", shiftKey: true });
    expect(document.activeElement).toBe(btn3);

    // Shift+Tab on last is a no-op (browser moves to previous element)
    btn2.focus(); // simulate browser default behavior
    fireEvent.keyDown(btn2, { key: "Tab", shiftKey: true });
    // Focus is still on btn2 (no wrap needed)
    expect(document.activeElement).toBe(btn2);

    // Shift+Tab on first again wraps to last
    btn1.focus();
    fireEvent.keyDown(btn1, { key: "Tab", shiftKey: true });
    expect(document.activeElement).toBe(btn3);
  });

  it("restores focus on unmount", () => {
    const { unmount } = render(<TrapHarness />);
    const btn1 = screen.getByTestId("btn1");
    expect(document.activeElement).toBe(btn1);

    // Unmount — focus should be restored to body
    unmount();
    expect(document.activeElement).toBe(document.body);
  });
});
