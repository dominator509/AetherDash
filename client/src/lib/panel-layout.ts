/**
 * Undockable panel layout system.
 *
 * Supports drag-to-undock, resize, pin, and preset layouts.
 * Manages the lifecycle and positioning of all trading panels
 * (order book, DOM, chart, etc.) within the AETHER Terminal.
 *
 * Plane 1 (client) — Zustand-compatible state manager.
 * No React dependency — usable from any consumer.
 */

// ── Types ──────────────────────────────────────────────────────────────────────

export type PanelId = string;

export type DockPosition = "left" | "right" | "bottom" | "center" | "floating";

export interface PanelConfig {
  id: PanelId;
  title: string;
  /** Component name to render — resolved by PanelLayout at runtime. */
  component: string;
  defaultDock: DockPosition;
  minWidth: number;
  minHeight: number;
  pinned: boolean;
}

export interface PanelInstance {
  config: PanelConfig;
  dock: DockPosition;
  x: number;
  y: number;
  width: number;
  height: number;
  zIndex: number;
  visible: boolean;
}

export interface PanelLayoutState {
  panels: Map<PanelId, PanelInstance>;
  /** Z-order — last entry renders on top. */
  order: PanelId[];
  maximized: PanelId | null;
  /** Previously saved layouts keyed by name. */
  presets: Record<string, PanelConfig[]>;
}

// ── Factory ────────────────────────────────────────────────────────────────────

export function createPanelLayout(): PanelLayoutState {
  return { panels: new Map(), order: [], maximized: null, presets: {} };
}

// ── Mutations (all return new snapshot for immutable consumers) ─────────────────

/**
 * Register a new panel from its configuration.
 * If a panel with the same id already exists, it is replaced.
 */
export function addPanel(state: PanelLayoutState, config: PanelConfig): PanelLayoutState {
  const existing = state.panels.get(config.id);
  const instance: PanelInstance = {
    config,
    dock: config.defaultDock,
    x: existing?.x ?? 0,
    y: existing?.y ?? 0,
    width: existing?.width ?? 400,
    height: existing?.height ?? 300,
    zIndex: existing?.zIndex ?? state.order.length,
    visible: existing?.visible ?? true,
  };
  state.panels.set(config.id, instance);
  if (!state.order.includes(config.id)) {
    state.order.push(config.id);
  }
  return state;
}

/**
 * Undock a panel — makes it floating and raises it to the top of the z-order.
 * No-op if panel does not exist.
 */
export function undockPanel(state: PanelLayoutState, id: PanelId): PanelLayoutState {
  const panel = state.panels.get(id);
  if (!panel) return state;

  panel.dock = "floating";
  panel.zIndex = getNextZIndex(state);
  bringToFront(state, id);
  return state;
}

/**
 * Dock (or re-dock) a floating panel to a specific dock position.
 */
export function redockPanel(
  state: PanelLayoutState,
  id: PanelId,
  position: DockPosition,
): PanelLayoutState {
  const panel = state.panels.get(id);
  if (!panel) return state;

  panel.dock = position;
  return state;
}

/**
 * Resize a panel. Clamps to the panel's configured minWidth/minHeight.
 */
export function resizePanel(
  state: PanelLayoutState,
  id: PanelId,
  w: number,
  h: number,
): PanelLayoutState {
  const panel = state.panels.get(id);
  if (!panel) return state;

  panel.width = Math.max(panel.config.minWidth, w);
  panel.height = Math.max(panel.config.minHeight, h);
  return state;
}

/**
 * Reposition a floating panel (drag).
 */
export function movePanel(
  state: PanelLayoutState,
  id: PanelId,
  x: number,
  y: number,
): PanelLayoutState {
  const panel = state.panels.get(id);
  if (!panel) return state;

  panel.x = x;
  panel.y = y;
  return state;
}

/**
 * Close (remove) a panel entirely.
 * If the panel is maximized, also clears maximized state.
 */
export function closePanel(state: PanelLayoutState, id: PanelId): PanelLayoutState {
  if (state.maximized === id) {
    state.maximized = null;
  }
  state.panels.delete(id);
  state.order = state.order.filter((i) => i !== id);
  return state;
}

/**
 * Toggle visibility of a panel (hide without removing).
 * Cannot hide a pinned panel.
 */
export function togglePanelVisibility(state: PanelLayoutState, id: PanelId): PanelLayoutState {
  const panel = state.panels.get(id);
  if (!panel || panel.config.pinned) return state;
  panel.visible = !panel.visible;
  return state;
}

/**
 * Maximize a panel to fill the layout. Pass null to restore all.
 */
export function maximizePanel(state: PanelLayoutState, id: PanelId | null): PanelLayoutState {
  if (id !== null && !state.panels.has(id)) return state;
  state.maximized = id;
  return state;
}

/**
 * Bring a panel to the top of the z-order.
 */
export function bringToFront(state: PanelLayoutState, id: PanelId): PanelLayoutState {
  const idx = state.order.indexOf(id);
  if (idx === -1) return state;
  state.order.splice(idx, 1);
  state.order.push(id);

  const panel = state.panels.get(id);
  if (panel) {
    panel.zIndex = getNextZIndex(state);
  }
  return state;
}

// ── Presets ────────────────────────────────────────────────────────────────────

/**
 * Save the current set of panel configs as a named preset.
 */
export function savePreset(state: PanelLayoutState, name: string): PanelLayoutState {
  const configs: PanelConfig[] = [];
  for (const [, instance] of state.panels) {
    configs.push({ ...instance.config });
  }
  state.presets[name] = configs;
  return state;
}

/**
 * Load a named preset, replacing all current panels.
 */
export function loadPreset(state: PanelLayoutState, name: string): PanelLayoutState {
  const configs = state.presets[name];
  if (!configs) return state;

  state.panels.clear();
  state.order = [];
  state.maximized = null;

  for (const config of configs) {
    addPanel(state, config);
  }
  return state;
}

/**
 * Get all currently registered preset names.
 */
export function listPresets(state: PanelLayoutState): string[] {
  return Object.keys(state.presets);
}

// ── Helpers ────────────────────────────────────────────────────────────────────

function getNextZIndex(state: PanelLayoutState): number {
  let max = 0;
  for (const [, p] of state.panels) {
    if (p.zIndex > max) max = p.zIndex;
  }
  return max + 1;
}
