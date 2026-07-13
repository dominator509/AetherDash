/**
 * AppFrame — main shell layout of AETHER Terminal.
 *
 * Left nav rail, top status bar, center surface host.
 * Wraps with Radix Tooltip provider for nav rail tooltips.
 *
 * Mode-based layout:
 * - Simple mode: single-column centered, max-width container
 * - Advanced mode: full-width layout with side panels
 *
 * Invariant INV-8: Mode switching MUST NOT alter data, subscriptions,
 * permissions, or pending confirms — only presentation changes.
 */

import * as Tooltip from "@radix-ui/react-tooltip";
import { useStore } from "../../state/store";
import { useKeyboardRouter } from "../../lib/keyboard";
import { NavRail } from "./NavRail";
import { StatusBar } from "./StatusBar";
import { SurfaceHost } from "./SurfaceHost";
import { CommandPalette } from "../palette/CommandPalette";
import { LoginScreen } from "./LoginScreen";

export function AppFrame() {
  const mode = useStore((s) => s.mode);
  const authenticated = useStore((s) => s.authenticated);

  // Mount global keyboard router (Ctrl/Cmd+K, Esc, surface delegation)
  useKeyboardRouter();

  // Show login screen when not authenticated
  if (!authenticated) {
    return <LoginScreen />;
  }

  const isSimple = mode === "simple";

  return (
    <Tooltip.Provider delayDuration={300}>
      <div className="flex h-screen flex-col bg-gray-950 text-gray-100">
        <StatusBar />
        <div className="flex flex-1 overflow-hidden">
          <NavRail />
          <div
            className={`flex flex-1 overflow-auto transition-all duration-200 ${
              isSimple ? "justify-center" : ""
            }`}
          >
            <div className={`w-full transition-all duration-200 ${isSimple ? "max-w-3xl" : ""}`}>
              <SurfaceHost />
            </div>
          </div>
        </div>
      </div>

      {/* Global command palette (controlled by store, not per-surface) */}
      <CommandPalette />
    </Tooltip.Provider>
  );
}
