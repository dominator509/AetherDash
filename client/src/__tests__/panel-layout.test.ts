/**
 * Tests for the panel layout manager (panel-layout.ts).
 *
 * Covers:
 * - Factory initial state
 * - addPanel / duplicate add
 * - undockPanel / redockPanel
 * - resizePanel with clamping
 * - movePanel (floating positioning)
 * - closePanel
 * - maximizePanel / restore
 * - bringToFront
 * - togglePanelVisibility (respects pinned)
 * - savePreset / loadPreset / listPresets
 */

import { describe, it, expect } from "vitest";
import {
  createPanelLayout,
  addPanel,
  undockPanel,
  redockPanel,
  resizePanel,
  movePanel,
  closePanel,
  maximizePanel,
  bringToFront,
  togglePanelVisibility,
  savePreset,
  loadPreset,
  listPresets,
} from "@/lib/panel-layout";
import type { PanelConfig } from "@/lib/panel-layout";

// ── Fixtures ───────────────────────────────────────────────────────────────────

const orderBookConfig: PanelConfig = {
  id: "orderbook",
  title: "Order Book",
  component: "OrderBook",
  defaultDock: "right",
  minWidth: 240,
  minHeight: 200,
  pinned: false,
};

const domConfig: PanelConfig = {
  id: "dom",
  title: "Depth of Market",
  component: "DepthOfMarket",
  defaultDock: "center",
  minWidth: 300,
  minHeight: 250,
  pinned: true,
};

const chartConfig: PanelConfig = {
  id: "chart",
  title: "Chart",
  component: "ChartPanel",
  defaultDock: "center",
  minWidth: 400,
  minHeight: 300,
  pinned: false,
};

// ── Tests ──────────────────────────────────────────────────────────────────────

describe("createPanelLayout", () => {
  it("returns empty state", () => {
    const state = createPanelLayout();
    expect(state.panels.size).toBe(0);
    expect(state.order).toEqual([]);
    expect(state.maximized).toBeNull();
    expect(state.presets).toEqual({});
  });
});

describe("addPanel", () => {
  it("registers a panel with the given config", () => {
    const state = createPanelLayout();
    addPanel(state, orderBookConfig);

    const panel = state.panels.get("orderbook");
    expect(panel).toBeDefined();
    expect(panel!.config.title).toBe("Order Book");
    expect(panel!.dock).toBe("right");
    expect(panel!.visible).toBe(true);
    expect(state.order).toEqual(["orderbook"]);
  });

  it("adds multiple panels in order", () => {
    const state = createPanelLayout();
    addPanel(state, orderBookConfig);
    addPanel(state, domConfig);
    addPanel(state, chartConfig);

    expect(state.panels.size).toBe(3);
    expect(state.order).toEqual(["orderbook", "dom", "chart"]);
  });

  it("does not duplicate id in order when panel already exists", () => {
    const state = createPanelLayout();
    addPanel(state, orderBookConfig);
    const prevLen = state.order.length;

    addPanel(state, { ...orderBookConfig, title: "Updated OB" });
    const panel = state.panels.get("orderbook");
    expect(panel!.config.title).toBe("Updated OB");
    expect(state.order.length).toBe(prevLen);
  });
});

describe("undockPanel", () => {
  it("sets dock to floating and assigns z-index", () => {
    const state = createPanelLayout();
    addPanel(state, orderBookConfig);
    undockPanel(state, "orderbook");

    const panel = state.panels.get("orderbook");
    expect(panel!.dock).toBe("floating");
    expect(panel!.zIndex).toBeGreaterThan(0);
  });

  it("is a no-op for non-existent panel", () => {
    const state = createPanelLayout();
    const result = undockPanel(state, "nonexistent");
    expect(result).toBe(state); // Same reference
  });

  it("raises floating panel on each undock", () => {
    const state = createPanelLayout();
    addPanel(state, orderBookConfig);
    addPanel(state, chartConfig);

    undockPanel(state, "orderbook");
    const firstZ = state.panels.get("orderbook")!.zIndex;

    undockPanel(state, "chart");
    const chartZ = state.panels.get("chart")!.zIndex;

    expect(chartZ).toBeGreaterThan(firstZ);
  });

  it("moves undocked panel to end of order", () => {
    const state = createPanelLayout();
    addPanel(state, orderBookConfig);
    addPanel(state, domConfig);
    addPanel(state, chartConfig);

    undockPanel(state, "orderbook");
    expect(state.order[state.order.length - 1]).toBe("orderbook");
  });
});

describe("redockPanel", () => {
  it("docks a floating panel to a specified position", () => {
    const state = createPanelLayout();
    addPanel(state, orderBookConfig);
    undockPanel(state, "orderbook");
    expect(state.panels.get("orderbook")!.dock).toBe("floating");

    redockPanel(state, "orderbook", "left");
    expect(state.panels.get("orderbook")!.dock).toBe("left");
  });

  it("changes dock position of a docked panel", () => {
    const state = createPanelLayout();
    addPanel(state, orderBookConfig);
    expect(state.panels.get("orderbook")!.dock).toBe("right");

    redockPanel(state, "orderbook", "bottom");
    expect(state.panels.get("orderbook")!.dock).toBe("bottom");
  });

  it("is a no-op for non-existent panel", () => {
    const state = createPanelLayout();
    const result = redockPanel(state, "nonexistent", "center");
    expect(result).toBe(state);
  });
});

describe("resizePanel", () => {
  it("updates width and height", () => {
    const state = createPanelLayout();
    addPanel(state, orderBookConfig);
    resizePanel(state, "orderbook", 500, 400);

    const panel = state.panels.get("orderbook");
    expect(panel!.width).toBe(500);
    expect(panel!.height).toBe(400);
  });

  it("clamps to minWidth", () => {
    const state = createPanelLayout();
    addPanel(state, orderBookConfig);
    resizePanel(state, "orderbook", 10, 400); // minWidth=240

    const panel = state.panels.get("orderbook");
    expect(panel!.width).toBe(240);
  });

  it("clamps to minHeight", () => {
    const state = createPanelLayout();
    addPanel(state, orderBookConfig);
    resizePanel(state, "orderbook", 400, 10); // minHeight=200

    const panel = state.panels.get("orderbook");
    expect(panel!.height).toBe(200);
  });

  it("is a no-op for non-existent panel", () => {
    const state = createPanelLayout();
    const result = resizePanel(state, "nonexistent", 500, 400);
    expect(result).toBe(state);
  });
});

describe("movePanel", () => {
  it("updates x and y coordinates", () => {
    const state = createPanelLayout();
    addPanel(state, orderBookConfig);
    undockPanel(state, "orderbook");
    movePanel(state, "orderbook", 150, 200);

    const panel = state.panels.get("orderbook");
    expect(panel!.x).toBe(150);
    expect(panel!.y).toBe(200);
  });

  it("is a no-op for non-existent panel", () => {
    const state = createPanelLayout();
    const result = movePanel(state, "nonexistent", 100, 100);
    expect(result).toBe(state);
  });
});

describe("closePanel", () => {
  it("removes the panel from state", () => {
    const state = createPanelLayout();
    addPanel(state, orderBookConfig);
    expect(state.panels.size).toBe(1);

    closePanel(state, "orderbook");
    expect(state.panels.size).toBe(0);
    expect(state.order).not.toContain("orderbook");
  });

  it("clears maximized state when closing maximized panel", () => {
    const state = createPanelLayout();
    addPanel(state, orderBookConfig);
    maximizePanel(state, "orderbook");
    expect(state.maximized).toBe("orderbook");

    closePanel(state, "orderbook");
    expect(state.maximized).toBeNull();
  });

  it("does not affect other panels", () => {
    const state = createPanelLayout();
    addPanel(state, orderBookConfig);
    addPanel(state, domConfig);

    closePanel(state, "orderbook");
    expect(state.panels.size).toBe(1);
    expect(state.panels.has("dom")).toBe(true);
    expect(state.order).toEqual(["dom"]);
  });

  it("is a no-op for non-existent panel", () => {
    const state = createPanelLayout();
    addPanel(state, orderBookConfig);
    const result = closePanel(state, "nonexistent");
    expect(result).toBe(state);
    expect(state.panels.size).toBe(1);
  });
});

describe("maximizePanel", () => {
  it("sets maximized to the panel id", () => {
    const state = createPanelLayout();
    addPanel(state, orderBookConfig);
    maximizePanel(state, "orderbook");
    expect(state.maximized).toBe("orderbook");
  });

  it("restores all when passed null", () => {
    const state = createPanelLayout();
    addPanel(state, orderBookConfig);
    maximizePanel(state, "orderbook");
    maximizePanel(state, null);
    expect(state.maximized).toBeNull();
  });

  it("is a no-op for non-existent panel", () => {
    const state = createPanelLayout();
    addPanel(state, orderBookConfig);
    const result = maximizePanel(state, "nonexistent");
    expect(result).toBe(state);
    expect(state.maximized).toBeNull();
  });
});

describe("bringToFront", () => {
  it("moves panel to end of order", () => {
    const state = createPanelLayout();
    addPanel(state, orderBookConfig);
    addPanel(state, domConfig);
    addPanel(state, chartConfig);

    bringToFront(state, "orderbook");
    expect(state.order[state.order.length - 1]).toBe("orderbook");
  });

  it("increases the panel zIndex", () => {
    const state = createPanelLayout();
    addPanel(state, orderBookConfig);
    addPanel(state, domConfig);

    const before = state.panels.get("orderbook")!.zIndex;
    bringToFront(state, "orderbook");
    const after = state.panels.get("orderbook")!.zIndex;
    expect(after).toBeGreaterThan(before);
  });

  it("is a no-op for non-existent panel", () => {
    const state = createPanelLayout();
    addPanel(state, orderBookConfig);
    const orderBefore = [...state.order];
    const result = bringToFront(state, "nonexistent");
    expect(result).toBe(state);
    expect(state.order).toEqual(orderBefore);
  });
});

describe("togglePanelVisibility", () => {
  it("hides a visible panel", () => {
    const state = createPanelLayout();
    addPanel(state, orderBookConfig);
    expect(state.panels.get("orderbook")!.visible).toBe(true);

    togglePanelVisibility(state, "orderbook");
    expect(state.panels.get("orderbook")!.visible).toBe(false);
  });

  it("shows a hidden panel", () => {
    const state = createPanelLayout();
    addPanel(state, orderBookConfig);
    togglePanelVisibility(state, "orderbook");
    expect(state.panels.get("orderbook")!.visible).toBe(false);

    togglePanelVisibility(state, "orderbook");
    expect(state.panels.get("orderbook")!.visible).toBe(true);
  });

  it("cannot hide a pinned panel", () => {
    const state = createPanelLayout();
    addPanel(state, domConfig); // pinned: true
    expect(state.panels.get("dom")!.config.pinned).toBe(true);

    togglePanelVisibility(state, "dom");
    expect(state.panels.get("dom")!.visible).toBe(true); // Still visible
  });

  it("is a no-op for non-existent panel", () => {
    const state = createPanelLayout();
    const result = togglePanelVisibility(state, "nonexistent");
    expect(result).toBe(state);
  });

  it("keeps at least one panel visible (via pinned guard)", () => {
    const state = createPanelLayout();
    addPanel(state, domConfig); // pinned=true, can't hide
    togglePanelVisibility(state, "dom");
    expect(state.panels.get("dom")!.visible).toBe(true);
  });
});

describe("savePreset / loadPreset / listPresets", () => {
  it("savePreset stores current panel configs by name", () => {
    const state = createPanelLayout();
    addPanel(state, orderBookConfig);
    addPanel(state, chartConfig);

    savePreset(state, "trading");
    const preset = state.presets["trading"];
    expect(preset).toBeDefined();
    expect(preset!.length).toBe(2);
  });

  it("loadPreset replaces all panels with saved configs", () => {
    const state = createPanelLayout();
    addPanel(state, orderBookConfig);
    addPanel(state, chartConfig);
    savePreset(state, "trading");

    // Clear and add other panel
    state.panels.clear();
    state.order = [];
    addPanel(state, domConfig);

    loadPreset(state, "trading");
    expect(state.panels.size).toBe(2);
    expect(state.panels.has("orderbook")).toBe(true);
    expect(state.panels.has("chart")).toBe(true);
    expect(state.panels.has("dom")).toBe(false);
  });

  it("loadPreset is a no-op for unknown preset name", () => {
    const state = createPanelLayout();
    addPanel(state, orderBookConfig);
    const result = loadPreset(state, "nonexistent");
    expect(result).toBe(state);
    expect(state.panels.size).toBe(1);
  });

  it("listPresets returns preset names", () => {
    const state = createPanelLayout();
    addPanel(state, orderBookConfig);
    savePreset(state, "trading");
    savePreset(state, "charting");
    addPanel(state, chartConfig);
    savePreset(state, "full");

    const names = listPresets(state);
    expect(names).toContain("trading");
    expect(names).toContain("charting");
    expect(names).toContain("full");
    expect(names.length).toBe(3);
  });

  it("savePreset stores configs even after panels are modified", () => {
    const state = createPanelLayout();
    addPanel(state, orderBookConfig);
    savePreset(state, "trading");

    // Modify the original panel
    const panel = state.panels.get("orderbook")!;
    panel.width = 999;

    // Preset should have the original config
    const saved = state.presets["trading"]?.[0];
    expect(saved?.minWidth).toBe(240);
    expect(saved?.defaultDock).toBe("right");
  });
});
