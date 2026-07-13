/**
 * Accessibility baseline tests for AETHER Terminal components.
 *
 * Verifies:
 * - Icon-only elements have aria-labels
 * - Status indicators have text or icon alongside color
 * - ARIA roles and properties are set correctly
 * - No element uses color as the ONLY signal
 */

import { describe, it, expect, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import * as Tooltip from "@radix-ui/react-tooltip";
import { AppFrame } from "../shell/AppFrame";
import { NavRail } from "../shell/NavRail";
import { StatusBar } from "../shell/StatusBar";
import { ErrorState } from "../states/ErrorState";
import { LoadingSkeleton } from "../states/LoadingSkeleton";
import { EmptyState } from "../states/EmptyState";
import { DegradationBanner } from "../states/DegradationBanner";
import { ModeToggle } from "../toggle/ModeToggle";
import { useStore } from "../../state/store";

// ── AppFrame ────────────────────────────────────────────────────────────────

describe("AppFrame accessibility", () => {
  beforeEach(() => {
    // AppFrame renders the navigation shell only when connected + authenticated
    useStore.setState({
      connectionStatus: "connected",
      activeSurface: "feed",
      authenticated: true,
    });
  });

  it("renders navigation landmark", () => {
    render(<AppFrame />);
    expect(screen.getByRole("navigation")).toBeTruthy();
  });

  it("renders header landmark", () => {
    const { container } = render(<AppFrame />);
    expect(container.querySelector("header")).toBeTruthy();
  });

  it("renders main landmark", () => {
    const { container } = render(<AppFrame />);
    expect(container.querySelector("main")).toBeTruthy();
  });
});

// ── NavRail ─────────────────────────────────────────────────────────────────

describe("NavRail accessibility", () => {
  beforeEach(() => {
    useStore.getState().setActiveSurface("feed");
  });

  it("all nav buttons have aria-label", () => {
    render(
      <Tooltip.Provider>
        <NavRail />
      </Tooltip.Provider>,
    );
    const labels = [
      "Feed",
      "Explain",
      "Simulate",
      "Ticket",
      "Command",
      "Alerts",
      "Positions",
      "Settings",
    ];
    for (const label of labels) {
      const btn = screen.getByLabelText(label);
      expect(btn).toBeTruthy();
      expect(btn.tagName).toBe("BUTTON");
    }
  });

  it("active surface button has aria-current='page'", () => {
    useStore.getState().setActiveSurface("explain");
    render(
      <Tooltip.Provider>
        <NavRail />
      </Tooltip.Provider>,
    );
    const activeBtn = screen.getByLabelText("Explain");
    expect(activeBtn.getAttribute("aria-current")).toBe("page");
  });

  it("inactive surfaces do not have aria-current", () => {
    useStore.getState().setActiveSurface("feed");
    render(
      <Tooltip.Provider>
        <NavRail />
      </Tooltip.Provider>,
    );
    const inactiveBtn = screen.getByLabelText("Settings");
    expect(inactiveBtn.getAttribute("aria-current")).toBeFalsy();
  });
});

// ── StatusBar accessibility ─────────────────────────────────────────────────

describe("StatusBar accessibility", () => {
  beforeEach(() => {
    useStore.getState().setConnectionStatus("disconnected");
    useStore.getState().setTier(null);
    useStore.getState().clearDegradations();
  });

  it("connection indicator has text label alongside color dot", () => {
    useStore.getState().setConnectionStatus("connected");
    render(<StatusBar />);
    // Must have visible text, not just a colored dot
    expect(screen.getByText("Connected")).toBeTruthy();
  });

  it("shows 'Disconnected' text when disconnected", () => {
    render(<StatusBar />);
    expect(screen.getByText("Disconnected")).toBeTruthy();
  });

  it("shows 'Connecting' text when connecting", () => {
    useStore.getState().setConnectionStatus("connecting");
    render(<StatusBar />);
    expect(screen.getByText("Connecting")).toBeTruthy();
  });

  it("tier badge has text label (not just number)", () => {
    useStore.getState().setTier(3);
    render(<StatusBar />);
    // The badge shows "T3" — the "T" prefix is the text label
    expect(screen.getByText("T3")).toBeTruthy();
  });

  it("degradation banner area has role='status' and aria-live='polite'", () => {
    useStore.getState().addDegradation({
      surface: "feed",
      reason: "Test degradation",
      started_at: "2026-07-11T12:00:00.000Z",
    });
    const { container } = render(<StatusBar />);
    // Find the degradation container
    const statusContainers = container.querySelectorAll('[role="status"]');
    expect(statusContainers.length).toBeGreaterThanOrEqual(1);
    for (const el of statusContainers) {
      expect(el.getAttribute("aria-live")).toBe("polite");
    }
  });
});

// ── ModeToggle accessibility ────────────────────────────────────────────────

describe("ModeToggle accessibility", () => {
  beforeEach(() => {
    useStore.getState().setConnectionStatus("connected");
  });

  it("has aria-label on the root toggle group", () => {
    render(<ModeToggle />);
    const group = screen.getByRole("radiogroup");
    expect(group.getAttribute("aria-label")).toBe("Display mode");
  });

  it("each item has an aria-label", () => {
    render(<ModeToggle />);
    expect(screen.getByLabelText("Simple mode")).toBeTruthy();
    expect(screen.getByLabelText("Advanced mode")).toBeTruthy();
  });
});

// ── ErrorState accessibility ────────────────────────────────────────────────

describe("ErrorState accessibility", () => {
  it("has role='alert' for immediate announcement", () => {
    render(
      <ErrorState
        code="unavailable"
        message="Service is down"
        trace_id="01ABCDEFGHIJKLMNOPQRSTUVWX"
      />,
    );
    // The top-level container should have role="alert"
    // Using query for the outer alert role
    const alerts = screen.getAllByRole("alert");
    expect(alerts.length).toBeGreaterThanOrEqual(1);
  });

  it("copy button has aria-label", () => {
    render(<ErrorState code="unavailable" message="Service is down" trace_id="TRACE123" />);
    const copyBtn = screen.getByLabelText("Copy trace ID to clipboard");
    expect(copyBtn).toBeTruthy();
  });

  it("retry button is still accessible by its visible text", () => {
    render(
      <ErrorState
        code="unavailable"
        message="Service is down"
        trace_id="TRACE123"
        retryable={true}
        onRetry={() => {}}
      />,
    );
    const retryBtn = screen.getByRole("button", { name: "Retry" });
    expect(retryBtn).toBeTruthy();
  });
});

// ── LoadingSkeleton accessibility ───────────────────────────────────────────

describe("LoadingSkeleton accessibility", () => {
  it("has role='status'", () => {
    const { container } = render(<LoadingSkeleton />);
    expect(container.querySelector('[role="status"]')).toBeTruthy();
  });

  it("has aria-busy='true'", () => {
    const { container } = render(<LoadingSkeleton />);
    const el = container.querySelector('[role="status"]');
    expect(el?.getAttribute("aria-busy")).toBe("true");
  });

  it("has aria-label describing loading state", () => {
    const { container } = render(<LoadingSkeleton />);
    const el = container.querySelector('[role="status"]');
    expect(el?.getAttribute("aria-label")).toBe("Loading content");
  });
});

// ── EmptyState accessibility ────────────────────────────────────────────────

describe("EmptyState accessibility", () => {
  it("has aria-label describing the empty state", () => {
    render(<EmptyState message="No markets available" />);
    const container = screen.getByLabelText("No markets available");
    expect(container).toBeTruthy();
  });

  it("action button is accessible by text", () => {
    render(<EmptyState message="Nothing here" actionLabel="Connect" onAction={() => {}} />);
    expect(screen.getByRole("button", { name: "Connect" })).toBeTruthy();
  });
});

// ── DegradationBanner accessibility ────────────────────────────────────────

describe("DegradationBanner accessibility", () => {
  it("has role='status' (not alert — degradation is not an error)", () => {
    render(
      <DegradationBanner
        surface="feed"
        reason="Upstream venue latency spike"
        started_at="2026-07-11T12:00:00.000Z"
      />,
    );
    const banners = screen.getAllByRole("status");
    expect(banners.length).toBeGreaterThanOrEqual(1);
  });

  it("has aria-live='polite'", () => {
    const { container } = render(
      <DegradationBanner surface="feed" reason="Test" started_at="2026-07-11T12:00:00.000Z" />,
    );
    const banners = container.querySelectorAll('[aria-live="polite"]');
    expect(banners.length).toBeGreaterThanOrEqual(1);
  });

  it("has visible text content alongside any color", () => {
    render(
      <DegradationBanner
        surface="feed"
        reason="Latency spike detected"
        started_at="2026-07-11T12:00:00.000Z"
      />,
    );
    // Color is not the only signal — text content is visible
    expect(screen.getByText("feed")).toBeTruthy();
    expect(screen.getByText("Latency spike detected")).toBeTruthy();
  });
});

// ── No color-only signaling ─────────────────────────────────────────────────

describe("no color-only signaling across components", () => {
  it("NavRail button icons have aria-hidden (not relied upon)", () => {
    render(
      <Tooltip.Provider>
        <NavRail />
      </Tooltip.Provider>,
    );
    // Icons are aria-hidden, meaning they're not the only signal — text labels exist
    const icons = document.querySelectorAll("svg[aria-hidden='true']");
    expect(icons.length).toBeGreaterThanOrEqual(8); // 8 surface icons
  });

  it("connection status has visible text label (not just colored dot)", () => {
    useStore.getState().setConnectionStatus("connected");
    render(<StatusBar />);
    const text = screen.getByText("Connected");
    expect(text).toBeTruthy();
    // The text node is a sibling of the colored dot span, not replacing it
    expect(text.tagName).toBe("SPAN");
  });

  it("tier badge has text prefix 'T' alongside any color", () => {
    useStore.getState().setTier(3);
    render(<StatusBar />);
    const badge = screen.getByText("T3");
    expect(badge).toBeTruthy();
    // Badge has bg-gray-800 color class AND visible text content
  });
});

// ── Focus trap integration ──────────────────────────────────────────────────

describe("focus management", () => {
  beforeEach(() => {
    useStore.setState({ authenticated: true });
  });

  it("all focusable elements use DOM order (no positive tabindex)", () => {
    render(<AppFrame />);
    const allElements = document.querySelectorAll("[tabindex]");
    allElements.forEach((el) => {
      const val = el.getAttribute("tabindex");
      if (val !== null && val !== "-1" && val !== "0") {
        // Positive tabindex values are not allowed
        expect(val).toMatch(/^-1$|^0$/);
      }
    });
  });
});
