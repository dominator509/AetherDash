/**
 * ModeToggle — a segmented control for switching between Simple and Advanced modes.
 *
 * Uses Radix Toggle Group primitive for accessible, keyboard-navigable toggle.
 * The Ctrl/Cmd+. (period) keyboard shortcut is owned by useKeyboardRouter in
 * keyboard.ts — this component only handles click events on toggle buttons.
 *
 * Mode is a UI presentation flag (INV-8). Switching is always allowed
 * regardless of connection state — the mode does not require a live
 * gateway connection.
 *
 * @see SPEC-004 INV-8
 * @see modeInvariant.ts
 */

import { useCallback } from "react";
import * as ToggleGroup from "@radix-ui/react-toggle-group";
import type { Mode } from "../../state/store";
import { useStore } from "../../state/store";
import { captureStateBeforeToggle, verifyStateAfterToggle } from "../../lib/modeInvariant";

// ── Props ──────────────────────────────────────────────────────────────────────

interface ModeToggleProps {
  /** Additional class names. */
  className?: string;
}

// ── Styles ──────────────────────────────────────────────────────────────────────

const ROOT_CLASSES =
  "inline-flex items-center gap-0 rounded-full border border-gray-700 bg-gray-900 p-0.5 transition-colors duration-150";

const ITEM_BASE =
  "relative z-10 rounded-full px-3 py-0.5 text-[11px] font-medium leading-none transition-all duration-150 select-none focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:ring-offset-1 focus-visible:ring-offset-gray-900";

const ITEM_ACTIVE = "text-gray-50";
const ITEM_INACTIVE = "text-gray-500 hover:text-gray-300 cursor-pointer";

// ── Animated indicator ──────────────────────────────────────────────────────────

const INDICATOR_CLASSES =
  "absolute inset-0 z-0 rounded-full bg-blue-600 transition-all duration-200 ease-out";

// ── Component ───────────────────────────────────────────────────────────────────

export function ModeToggle({ className = "" }: ModeToggleProps) {
  const mode = useStore((s) => s.mode);
  const setMode = useStore((s) => s.setMode);

  const handleModeChange = useCallback(
    (value: string) => {
      if (!value || value === mode) return;

      // INV-8: Capture state before toggle
      const before = captureStateBeforeToggle();

      setMode(value as Mode);

      // INV-8: Verify state after toggle
      const after = captureStateBeforeToggle();
      const violations = verifyStateAfterToggle(before, after);
      if (violations.length > 0) {
        console.warn("[ModeToggle] INV-8 violations detected:", violations);
      }
    },
    [mode, setMode],
  );

  return (
    <ToggleGroup.Root
      type="single"
      value={mode}
      onValueChange={handleModeChange}
      className={`${ROOT_CLASSES} ${className}`}
      aria-label="Display mode"
    >
      <div className="relative flex items-center">
        {/* Sliding indicator */}
        <span
          className={INDICATOR_CLASSES}
          style={{
            transform: mode === "simple" ? "translateX(0)" : "translateX(100%)",
            width: "50%",
          }}
        />

        <ToggleGroup.Item
          value="simple"
          className={`${ITEM_BASE} ${mode === "simple" ? ITEM_ACTIVE : ITEM_INACTIVE}`}
          aria-label="Simple mode"
          title="Switch to Simple mode"
        >
          Simple
        </ToggleGroup.Item>

        <ToggleGroup.Item
          value="advanced"
          className={`${ITEM_BASE} ${mode === "advanced" ? ITEM_ACTIVE : ITEM_INACTIVE}`}
          aria-label="Advanced mode"
          title="Switch to Advanced mode"
        >
          Advanced
        </ToggleGroup.Item>
      </div>
    </ToggleGroup.Root>
  );
}
