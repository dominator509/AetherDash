/**
 * Undockable panel layout container (SPEC-004).
 *
 * Renders panels in their docked (left, right, bottom, center) or
 * floating positions with drag-to-undock, resize handles, and
 * z-order management.
 *
 * Usage:
 * ```tsx
 * const [state, setState] = useState(createPanelLayout());
 * // ... add panels via addPanel(state, config) ...
 * <PanelLayout state={state} onChange={setState} registry={componentMap} />
 * ```
 */

import React, { useState, useCallback, useRef, useEffect, useMemo } from "react";
import type { PanelLayoutState, PanelId, PanelInstance, DockPosition } from "@/lib/panel-layout";
import {
  undockPanel,
  closePanel,
  maximizePanel,
  resizePanel,
  movePanel,
  bringToFront,
} from "@/lib/panel-layout";

// ── Types ──────────────────────────────────────────────────────────────────────

/** Maps component name strings from PanelConfig to actual React components. */
export type PanelComponentRegistry = Record<string, React.ComponentType<Record<string, unknown>>>;

export interface PanelLayoutProps {
  /** Current panel layout state. */
  state: PanelLayoutState;
  /** Callback when the layout state mutates. */
  onChange: (state: PanelLayoutState) => void;
  /** Registry mapping component names to React components. */
  registry: PanelComponentRegistry;
  /** Optional className for the outer container. */
  className?: string;
}

// ── Drag State ─────────────────────────────────────────────────────────────────

interface DragState {
  type: "move" | "resize";
  panelId: PanelId;
  startX: number;
  startY: number;
  startW: number;
  startH: number;
  startPanelX: number;
  startPanelY: number;
  handle?: "nw" | "ne" | "sw" | "se" | "n" | "s" | "e" | "w";
}

// ── Dock layout constants ──────────────────────────────────────────────────────

const DOCK_LEFT_WIDTH = 360;
const DOCK_RIGHT_WIDTH = 360;
const DOCK_BOTTOM_HEIGHT = 280;

// ── Component ──────────────────────────────────────────────────────────────────

export function PanelLayout({ state, onChange, registry, className = "" }: PanelLayoutProps) {
  const [dragState, setDragState] = useState<DragState | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  // ---- Derived panel groups ----

  const { centerPanels, leftPanels, rightPanels, bottomPanels, floatingPanels } = useMemo(() => {
    const groups: Record<DockPosition, PanelInstance[]> = {
      center: [],
      left: [],
      right: [],
      bottom: [],
      floating: [],
    };

    for (const id of state.order) {
      const panel = state.panels.get(id);
      if (!panel || !panel.visible) continue;
      groups[panel.dock]?.push(panel);
    }

    return {
      centerPanels: groups.center,
      leftPanels: groups.left,
      rightPanels: groups.right,
      bottomPanels: groups.bottom,
      floatingPanels: groups.floating,
    };
  }, [state]);

  // When maximized, only render that panel
  const maximizedPanel = state.maximized ? state.panels.get(state.maximized) : null;

  // ---- Clamp floating panels on mount ----

  useEffect(() => {
    let changed = false;
    for (const panel of floatingPanels) {
      if (panel.x < 0) {
        panel.x = 0;
        changed = true;
      }
      if (panel.y < 0) {
        panel.y = 0;
        changed = true;
      }
    }
    if (changed) {
      onChange({ ...state, panels: new Map(state.panels) });
    }
  }, []);

  // ---- Drag handlers ----

  const handleDragStart = useCallback(
    (
      e: React.MouseEvent | React.TouchEvent,
      panelId: PanelId,
      type: "move" | "resize",
      handle?: DragState["handle"],
    ) => {
      const panel = state.panels.get(panelId);
      if (!panel) return;

      e.preventDefault();

      let clientX: number;
      let clientY: number;
      if ("touches" in e) {
        const touch = e.touches[0];
        if (!touch) return;
        clientX = touch.clientX;
        clientY = touch.clientY;
      } else {
        clientX = e.clientX;
        clientY = e.clientY;
      }

      setDragState({
        type,
        panelId,
        startX: clientX,
        startY: clientY,
        startW: panel.width,
        startH: panel.height,
        startPanelX: panel.x,
        startPanelY: panel.y,
        handle,
      });
    },
    [state],
  );

  // Handle drag move
  useEffect(() => {
    if (!dragState) return;

    const handleMove = (e: MouseEvent | TouchEvent) => {
      let clientX: number;
      let clientY: number;
      if ("touches" in e) {
        const touch = e.touches[0];
        if (!touch) return;
        clientX = touch.clientX;
        clientY = touch.clientY;
      } else {
        clientX = e.clientX;
        clientY = e.clientY;
      }
      const dx = clientX - dragState.startX;
      const dy = clientY - dragState.startY;

      if (dragState.type === "move") {
        onChange(
          movePanel(
            state,
            dragState.panelId,
            dragState.startPanelX + dx,
            dragState.startPanelY + dy,
          ),
        );
      } else if (dragState.type === "resize") {
        const handle = dragState.handle ?? "se";
        let newW = dragState.startW;
        let newH = dragState.startH;

        if (handle.includes("e")) newW = dragState.startW + dx;
        if (handle.includes("w")) newW = dragState.startW - dx;
        if (handle.includes("s")) newH = dragState.startH + dy;
        if (handle.includes("n")) newH = dragState.startH - dy;

        onChange(resizePanel(state, dragState.panelId, Math.round(newW), Math.round(newH)));
      }
    };

    const handleUp = () => {
      setDragState(null);
    };

    window.addEventListener("mousemove", handleMove);
    window.addEventListener("mouseup", handleUp);
    window.addEventListener("touchmove", handleMove, { passive: false });
    window.addEventListener("touchend", handleUp);

    return () => {
      window.removeEventListener("mousemove", handleMove);
      window.removeEventListener("mouseup", handleUp);
      window.removeEventListener("touchmove", handleMove);
      window.removeEventListener("touchend", handleUp);
    };
  }, [dragState, state, onChange]);

  // ---- Event handlers ----

  const handleUndock = useCallback(
    (id: PanelId) => {
      onChange(undockPanel(state, id));
    },
    [state, onChange],
  );

  const handleClose = useCallback(
    (id: PanelId) => {
      onChange(closePanel(state, id));
    },
    [state, onChange],
  );

  const handleMaximize = useCallback(
    (id: PanelId) => {
      onChange(maximizePanel(state, state.maximized === id ? null : id));
    },
    [state, onChange],
  );

  const handleFocus = useCallback(
    (id: PanelId) => {
      const panel = state.panels.get(id);
      if (panel && panel.dock === "floating") {
        onChange(bringToFront(state, id));
      }
    },
    [state, onChange],
  );

  // ---- Render helpers ----

  const renderPanelContent = (panel: PanelInstance) => {
    const Component = registry[panel.config.component];
    return (
      <div className="flex flex-col h-full w-full">
        {renderPanelHeader(panel)}
        <div className="flex-1 overflow-hidden">
          {Component ? <Component /> : <MissingComponent name={panel.config.component} />}
        </div>
      </div>
    );
  };

  const renderPanelHeader = (panel: PanelInstance) => (
    <div
      className="flex items-center justify-between px-2 py-1 bg-gray-100 border-b border-gray-200 cursor-grab active:cursor-grabbing select-none flex-shrink-0"
      onMouseDown={(e) => panel.dock === "floating" && handleDragStart(e, panel.config.id, "move")}
      onDoubleClick={() => handleUndock(panel.config.id)}
    >
      <span className="text-xs font-medium text-gray-700 truncate">{panel.config.title}</span>
      <div className="flex items-center gap-0.5">
        {/* Undock button — only show for docked panels */}
        {panel.dock !== "floating" && (
          <PanelButton
            label="Undock"
            onClick={() => handleUndock(panel.config.id)}
            title="Undock panel"
          >
            <svg width="10" height="10" viewBox="0 0 10 10" fill="none">
              <rect
                x="1"
                y="1"
                width="6"
                height="6"
                rx="1"
                stroke="currentColor"
                strokeWidth="0.8"
              />
              <path d="M5 4 L9 8 M9 5 V9 H5" stroke="currentColor" strokeWidth="0.8" />
            </svg>
          </PanelButton>
        )}
        {/* Maximize button */}
        <PanelButton
          label={state.maximized === panel.config.id ? "Restore" : "Maximize"}
          onClick={() => handleMaximize(panel.config.id)}
          title={state.maximized === panel.config.id ? "Restore" : "Maximize"}
        >
          <svg width="10" height="10" viewBox="0 0 10 10" fill="none">
            {state.maximized === panel.config.id ? (
              <>
                <rect
                  x="3"
                  y="3"
                  width="6"
                  height="6"
                  rx="1"
                  stroke="currentColor"
                  strokeWidth="0.8"
                />
                <rect
                  x="0.5"
                  y="0.5"
                  width="4.5"
                  height="4.5"
                  rx="1"
                  stroke="currentColor"
                  strokeWidth="0.8"
                />
              </>
            ) : (
              <rect
                x="1"
                y="1"
                width="8"
                height="8"
                rx="1"
                stroke="currentColor"
                strokeWidth="0.8"
              />
            )}
          </svg>
        </PanelButton>
        {/* Close button — hidden for pinned panels */}
        {!panel.config.pinned && (
          <PanelButton
            label="Close"
            onClick={() => handleClose(panel.config.id)}
            title="Close panel"
          >
            <svg width="10" height="10" viewBox="0 0 10 10" fill="none">
              <path d="M2 2 L8 8 M8 2 L2 8" stroke="currentColor" strokeWidth="0.8" />
            </svg>
          </PanelButton>
        )}
      </div>
    </div>
  );

  // Floating panel resize handles (corners + edges)
  const renderFloatingResizeHandles = (panel: PanelInstance) => {
    const positions = ["nw", "ne", "sw", "se", "n", "s", "e", "w"] as const;
    return positions.map((pos) => (
      <div
        key={pos}
        className="absolute z-20"
        style={{
          cursor: getResizeCursor(pos),
          ...getResizeHandleStyle(pos),
        }}
        onMouseDown={(e) => handleDragStart(e, panel.config.id, "resize", pos)}
      />
    ));
  };

  // ---- Maximized view ----

  if (maximizedPanel) {
    return (
      <div
        ref={containerRef}
        className={`relative w-full h-full overflow-hidden bg-gray-950 ${className}`}
      >
        <div className="absolute inset-0 z-50 flex flex-col bg-white border-2 border-blue-400 rounded">
          {renderPanelContent(maximizedPanel)}
        </div>
      </div>
    );
  }

  // ---- Normal view ----

  const hasLeft = leftPanels.length > 0;
  const hasRight = rightPanels.length > 0;
  const hasBottom = bottomPanels.length > 0;

  return (
    <div
      ref={containerRef}
      className={`relative w-full h-full overflow-hidden bg-gray-950 ${className}`}
      data-layout="panel-layout"
    >
      {/* Grid-based dock layout */}
      <div
        className="w-full h-full"
        style={{
          display: "grid",
          gridTemplateColumns: hasLeft
            ? `${DOCK_LEFT_WIDTH}px 1fr${hasRight ? ` ${DOCK_RIGHT_WIDTH}px` : ""}`
            : `1fr${hasRight ? ` ${DOCK_RIGHT_WIDTH}px` : ""}`,
          gridTemplateRows: hasBottom ? `1fr ${DOCK_BOTTOM_HEIGHT}px` : "1fr",
          gridTemplateAreas: [hasLeft ? "left" : "", "center", hasRight ? "right" : ""]
            .filter(Boolean)
            .map((area) => {
              if (area === "left") return `"left center${hasRight ? " right" : ""}"`;
              if (area === "center") return `"left center${hasRight ? " right" : ""}"`;
              return "";
            })
            .filter(Boolean)
            .join(" "),
        }}
      >
        {/* Left dock */}
        {hasLeft && (
          <div
            className="flex flex-col gap-0.5 p-0.5 overflow-hidden bg-gray-900"
            style={{ gridArea: "left" }}
          >
            {leftPanels.map((panel) => (
              <DockedPanel key={panel.config.id} panel={panel} onFocus={handleFocus}>
                {renderPanelContent(panel)}
              </DockedPanel>
            ))}
          </div>
        )}

        {/* Center dock */}
        <div
          className="flex flex-col gap-0.5 p-0.5 overflow-hidden bg-gray-900"
          style={{ gridArea: "center" }}
        >
          {centerPanels.length === 0 && (
            <div className="flex items-center justify-center h-full text-gray-500 text-xs bg-gray-900 rounded border border-dashed border-gray-700">
              Drop panels here or undock a panel to float
            </div>
          )}
          {centerPanels.map((panel) => (
            <DockedPanel key={panel.config.id} panel={panel} onFocus={handleFocus}>
              {renderPanelContent(panel)}
            </DockedPanel>
          ))}
        </div>

        {/* Right dock */}
        {hasRight && (
          <div
            className="flex flex-col gap-0.5 p-0.5 overflow-hidden bg-gray-900"
            style={{ gridArea: "right" }}
          >
            {rightPanels.map((panel) => (
              <DockedPanel key={panel.config.id} panel={panel} onFocus={handleFocus}>
                {renderPanelContent(panel)}
              </DockedPanel>
            ))}
          </div>
        )}
      </div>

      {/* Bottom dock — rendered below the grid */}
      {hasBottom && (
        <div
          className="absolute bottom-0 left-0 right-0 flex gap-0.5 p-0.5 overflow-hidden bg-gray-900 border-t border-gray-700"
          style={{ height: DOCK_BOTTOM_HEIGHT }}
        >
          {bottomPanels.map((panel) => (
            <DockedPanel key={panel.config.id} panel={panel} onFocus={handleFocus}>
              {renderPanelContent(panel)}
            </DockedPanel>
          ))}
        </div>
      )}

      {/* Floating panels */}
      {floatingPanels.map((panel) => (
        <div
          key={panel.config.id}
          className="absolute flex flex-col bg-white border border-gray-300 rounded shadow-lg overflow-hidden"
          style={{
            left: panel.x,
            top: panel.y,
            width: panel.width,
            height: panel.height,
            zIndex: panel.zIndex,
          }}
          onMouseDown={() => handleFocus(panel.config.id)}
        >
          {renderPanelContent(panel)}
          {/* Resize corner handles */}
          <div
            className="absolute bottom-0 right-0 w-4 h-4 cursor-se-resize z-20"
            style={{ background: "linear-gradient(135deg, transparent 50%, rgba(0,0,0,0.15) 50%)" }}
            onMouseDown={(e) => handleDragStart(e, panel.config.id, "resize", "se")}
          />
          {/* Edge resize handles */}
          {renderFloatingResizeHandles(panel)}
        </div>
      ))}
    </div>
  );
}

// ── Sub-components ─────────────────────────────────────────────────────────────

interface DockedPanelProps {
  panel: PanelInstance;
  onFocus: (id: PanelId) => void;
  children: React.ReactNode;
}

function DockedPanel({ panel, onFocus, children }: DockedPanelProps) {
  return (
    <div
      className="flex flex-col flex-1 bg-white border border-gray-200 rounded overflow-hidden min-h-0"
      onMouseDown={() => onFocus(panel.config.id)}
      data-panel-id={panel.config.id}
    >
      {children}
    </div>
  );
}

interface PanelButtonProps {
  label: string;
  onClick: () => void;
  title: string;
  children: React.ReactNode;
}

function PanelButton({ label, onClick, title, children }: PanelButtonProps) {
  return (
    <button
      aria-label={label}
      title={title}
      className="p-0.5 text-gray-400 hover:text-gray-700 hover:bg-gray-200 rounded"
      onClick={(e) => {
        e.stopPropagation();
        onClick();
      }}
    >
      {children}
    </button>
  );
}

function MissingComponent({ name }: { name: string }) {
  return (
    <div className="flex h-full items-center justify-center text-gray-400 text-xs">
      Unknown component: {name}
    </div>
  );
}

// ── Resize handle geometry ─────────────────────────────────────────────────────

const HANDLE_HIT = 6;
const CORNER_SIZE = 12;

function getResizeCursor(pos: string): string {
  const map: Record<string, string> = {
    nw: "nwse-resize",
    ne: "nesw-resize",
    sw: "nesw-resize",
    se: "nwse-resize",
    n: "ns-resize",
    s: "ns-resize",
    e: "ew-resize",
    w: "ew-resize",
  };
  return map[pos] ?? "default";
}

function getResizeHandleStyle(pos: string): React.CSSProperties {
  const edge = HANDLE_HIT;

  switch (pos) {
    case "nw":
      return { top: 0, left: 0, width: CORNER_SIZE, height: CORNER_SIZE };
    case "ne":
      return { top: 0, right: 0, width: CORNER_SIZE, height: CORNER_SIZE };
    case "sw":
      return { bottom: 0, left: 0, width: CORNER_SIZE, height: CORNER_SIZE };
    case "se":
      return { bottom: 0, right: 0, width: CORNER_SIZE, height: CORNER_SIZE };
    case "n":
      return { top: 0, left: CORNER_SIZE, right: CORNER_SIZE, height: edge };
    case "s":
      return {
        bottom: 0,
        left: CORNER_SIZE,
        right: CORNER_SIZE,
        height: edge,
      };
    case "e":
      return {
        top: CORNER_SIZE,
        right: 0,
        bottom: CORNER_SIZE,
        width: edge,
      };
    case "w":
      return {
        top: CORNER_SIZE,
        left: 0,
        bottom: CORNER_SIZE,
        width: edge,
      };
    default:
      return {};
  }
}
