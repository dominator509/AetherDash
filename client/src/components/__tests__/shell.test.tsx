/**
 * Tests for shell components: AppFrame, NavRail, StatusBar, SurfaceHost.
 */

import { describe, it, expect, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import * as Tooltip from "@radix-ui/react-tooltip";
import { AppFrame } from "../shell/AppFrame";
import { NavRail } from "../shell/NavRail";
import { StatusBar } from "../shell/StatusBar";
import { useStore } from "../../state/store";

describe("AppFrame", () => {
  beforeEach(() => {
    // AppFrame renders the full navigation shell only when connected + authenticated
    useStore.setState({
      connectionStatus: "connected",
      activeSurface: "feed",
      authenticated: true,
    });
  });

  it("renders nav rail, status bar, and surface host", () => {
    const { container } = render(<AppFrame />);
    // Nav rail: navigation landmark
    expect(screen.getByRole("navigation")).toBeTruthy();
    // Status bar: header element
    expect(container.querySelector("header")).toBeTruthy();
    // Surface host: main element
    expect(container.querySelector("main")).toBeTruthy();
  });

  it("renders within a full-screen container", () => {
    const { container } = render(<AppFrame />);
    const outer = container.firstChild as HTMLElement;
    expect(outer.className).toContain("h-screen");
  });
});

describe("NavRail", () => {
  const surfaceLabels = [
    "Feed",
    "Explain",
    "Simulate",
    "Ticket",
    "Command",
    "Alerts",
    "Positions",
    "Settings",
  ];

  beforeEach(() => {
    // Reset active surface to feed
    useStore.getState().setActiveSurface("feed");
  });

  it("renders all 8 surface buttons", () => {
    render(
      <Tooltip.Provider>
        <NavRail />
      </Tooltip.Provider>,
    );
    for (const label of surfaceLabels) {
      expect(screen.getByLabelText(label)).toBeTruthy();
    }
  });

  it("highlights the active surface", () => {
    useStore.getState().setActiveSurface("explain");
    render(
      <Tooltip.Provider>
        <NavRail />
      </Tooltip.Provider>,
    );
    const btn = screen.getByLabelText("Explain");
    expect(btn.getAttribute("aria-current")).toBe("page");
  });

  it("does not highlight inactive surfaces", () => {
    useStore.getState().setActiveSurface("feed");
    render(
      <Tooltip.Provider>
        <NavRail />
      </Tooltip.Provider>,
    );
    const btn = screen.getByLabelText("Settings");
    expect(btn.getAttribute("aria-current")).toBeFalsy();
  });
});

describe("StatusBar", () => {
  beforeEach(() => {
    // Reset state
    useStore.getState().setConnectionStatus("disconnected");
    useStore.getState().setTier(null);
    useStore.getState().clearDegradations();
  });

  it("shows disconnected state by default", () => {
    render(<StatusBar />);
    expect(screen.getByText("Disconnected")).toBeTruthy();
  });

  it("shows connected state", () => {
    useStore.getState().setConnectionStatus("connected");
    render(<StatusBar />);
    expect(screen.getByText("Connected")).toBeTruthy();
  });

  it("shows connecting state", () => {
    useStore.getState().setConnectionStatus("connecting");
    render(<StatusBar />);
    expect(screen.getByText("Connecting")).toBeTruthy();
  });

  it("shows tier badge when tier is set", () => {
    useStore.getState().setTier(3);
    render(<StatusBar />);
    expect(screen.getByText("T3")).toBeTruthy();
  });

  it("does not show tier badge when tier is null", () => {
    render(<StatusBar />);
    expect(screen.queryByText(/^T\d/)).toBeNull();
  });
});
