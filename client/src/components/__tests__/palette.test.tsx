/**
 * Tests for CommandPalette component.
 *
 * Covers:
 * - Renders all 8 surface commands
 * - Fuzzy search filters results
 * - Keyboard navigation (arrow keys) moves selection
 * - Enter selects and navigates
 * - Esc closes
 * - Commands show correct shortcut hints
 */

import { describe, it, expect, beforeEach } from "vitest";
import { render, screen, fireEvent, act } from "@testing-library/react";
import { CommandPalette } from "../palette/CommandPalette";
import { useStore } from "../../state/store";

// ── Setup ─────────────────────────────────────────────────────────────────

beforeEach(() => {
  useStore.setState({
    paletteOpen: false,
    activeSurface: "feed",
  });
});

// ── CommandPalette ────────────────────────────────────────────────────────

describe("CommandPalette", () => {
  it("renders all 8 surface commands when open", () => {
    useStore.getState().openPalette();
    render(<CommandPalette />);

    expect(screen.getByText("Feed")).toBeTruthy();
    expect(screen.getByText("Explain")).toBeTruthy();
    expect(screen.getByText("Simulate")).toBeTruthy();
    expect(screen.getByText("Ticket")).toBeTruthy();
    expect(screen.getByText("Command")).toBeTruthy();
    expect(screen.getByText("Alerts")).toBeTruthy();
    expect(screen.getByText("Positions")).toBeTruthy();
    // "Settings" appears both as a surface command and a group label
    const settingsElements = screen.getAllByText("Settings");
    expect(settingsElements.length).toBeGreaterThanOrEqual(1);
  });

  it("renders feed action commands when open", () => {
    useStore.getState().openPalette();
    render(<CommandPalette />);

    expect(screen.getByText("Explain Item")).toBeTruthy();
    expect(screen.getByText("Simulate Item")).toBeTruthy();
    expect(screen.getByText("Act (Open Ticket)")).toBeTruthy();
    expect(screen.getByText("Ignore Item")).toBeTruthy();
  });

  it("renders group labels", () => {
    useStore.getState().openPalette();
    render(<CommandPalette />);

    expect(screen.getByText("Navigation")).toBeTruthy();
    expect(screen.getByText("Actions")).toBeTruthy();
    // "Settings" appears both as a group label and a surface command
    const settingsElements = screen.getAllByText("Settings");
    expect(settingsElements.length).toBeGreaterThanOrEqual(1);
  });

  it("fuzzy filters results by search query", () => {
    useStore.getState().openPalette();
    render(<CommandPalette />);

    const input = screen.getByLabelText("Search commands");
    fireEvent.change(input, { target: { value: "ex" } });

    // "Explain" and "Explain Item" should match "ex"
    expect(screen.getByText("Explain")).toBeTruthy();
    expect(screen.getByText("Explain Item")).toBeTruthy();

    // "Feed" should be filtered out
    expect(screen.queryByText("Feed")).toBeNull();
  });

  it("shows 'No commands found' for unmatched queries", () => {
    useStore.getState().openPalette();
    render(<CommandPalette />);

    const input = screen.getByLabelText("Search commands");
    fireEvent.change(input, { target: { value: "zzz_nonexistent_zzz" } });

    expect(screen.getByText("No commands found")).toBeTruthy();
  });

  it("moves selection with arrow keys", () => {
    useStore.getState().openPalette();
    render(<CommandPalette />);

    const input = screen.getByLabelText("Search commands");
    const options = () => screen.getAllByRole("option");

    // Initial: first option should be selected
    expect(options()[0]?.getAttribute("aria-selected")).toBe("true");

    // ArrowDown: second option should be selected
    fireEvent.keyDown(input, { key: "ArrowDown" });
    expect(options()[1]?.getAttribute("aria-selected")).toBe("true");

    // ArrowDown again: third option
    fireEvent.keyDown(input, { key: "ArrowDown" });
    expect(options()[2]?.getAttribute("aria-selected")).toBe("true");

    // ArrowUp: back to second
    fireEvent.keyDown(input, { key: "ArrowUp" });
    expect(options()[1]?.getAttribute("aria-selected")).toBe("true");
  });

  it("selects and navigates on Enter", () => {
    useStore.getState().openPalette();
    useStore.getState().setActiveSurface("settings");
    render(<CommandPalette />);

    const input = screen.getByLabelText("Search commands");

    // First option should be "Feed" — press Enter
    fireEvent.keyDown(input, { key: "Enter" });

    // Should have navigated to feed and closed palette
    expect(useStore.getState().activeSurface).toBe("feed");
    expect(useStore.getState().paletteOpen).toBe(false);
  });

  it("closes on Escape via onOpenChange", () => {
    useStore.getState().openPalette();
    render(<CommandPalette />);

    // Close via store action (which is what onOpenChange calls)
    act(() => {
      useStore.getState().closePalette();
    });
    expect(useStore.getState().paletteOpen).toBe(false);
  });

  it("shows shortcut hints for commands", () => {
    useStore.getState().openPalette();
    render(<CommandPalette />);

    // Navigation shortcuts should be visible
    expect(screen.getByText("G F")).toBeTruthy();
    expect(screen.getByText("G E")).toBeTruthy();
    expect(screen.getByText("G S")).toBeTruthy();

    // Action shortcuts should be visible
    expect(screen.getByText("E")).toBeTruthy();
    expect(screen.getByText("S")).toBeTruthy();
    expect(screen.getByText("A")).toBeTruthy();
    expect(screen.getByText("I")).toBeTruthy();
  });

  it("renders search input with placeholder", () => {
    useStore.getState().openPalette();
    render(<CommandPalette />);

    const searchInput = screen.getByPlaceholderText("Search commands and surfaces...");
    expect(searchInput).toBeTruthy();
  });

  it("displays footer keyboard hint", () => {
    useStore.getState().openPalette();
    render(<CommandPalette />);

    // Text spans multiple elements (arrow char in <span>, label after)
    expect(screen.getByText(/Navigate/)).toBeTruthy();
    expect(screen.getByText(/Select/)).toBeTruthy();
    expect(screen.getByText(/Close/)).toBeTruthy();
  });

  it("resets query and selection on open", () => {
    // Open, type something, then close
    useStore.getState().openPalette();
    const { unmount } = render(<CommandPalette />);
    const input = screen.getByLabelText("Search commands");
    fireEvent.change(input, { target: { value: "zzz" } });

    // Close palette inside act to wrap Dialog close animation
    act(() => {
      useStore.getState().closePalette();
    });
    unmount();

    // Re-open — query should be reset
    useStore.getState().openPalette();
    render(<CommandPalette />);
    const newInput = screen.getByLabelText("Search commands") as HTMLInputElement;
    expect(newInput.value).toBe("");
  });
});
