/**
 * CommandRegistry — central registry of palette commands.
 *
 * All commands are registered here. Surfaces register navigation commands,
 * and feed actions are registered as stub actions for EP-102+.
 */

import type { SurfaceName } from "../../state/store";

// ── Types ──────────────────────────────────────────────────────────────────

export interface Command {
  id: string;
  label: string;
  shortcut?: string;
  surface?: SurfaceName;
  action?: () => void;
  group: "navigation" | "actions" | "settings";
}

// ── Registry ───────────────────────────────────────────────────────────────

const commands: Command[] = [
  // Navigation — all 8 surfaces
  { id: "nav-feed", label: "Feed", shortcut: "G F", surface: "feed", group: "navigation" },
  { id: "nav-explain", label: "Explain", shortcut: "G E", surface: "explain", group: "navigation" },
  {
    id: "nav-simulate",
    label: "Simulate",
    shortcut: "G S",
    surface: "simulate",
    group: "navigation",
  },
  { id: "nav-ticket", label: "Ticket", shortcut: "G T", surface: "ticket", group: "navigation" },
  { id: "nav-command", label: "Command", shortcut: "G C", surface: "command", group: "navigation" },
  { id: "nav-alerts", label: "Alerts", shortcut: "G A", surface: "alerts", group: "navigation" },
  {
    id: "nav-positions",
    label: "Positions",
    shortcut: "G P",
    surface: "positions",
    group: "navigation",
  },
  {
    id: "nav-settings",
    label: "Settings",
    shortcut: "G ,",
    surface: "settings",
    group: "settings",
  },

  // Feed actions (stubs — real interaction in EP-102)
  { id: "action-explain", label: "Explain Item", shortcut: "E", group: "actions" },
  { id: "action-simulate", label: "Simulate Item", shortcut: "S", group: "actions" },
  { id: "action-act", label: "Act (Open Ticket)", shortcut: "A", group: "actions" },
  { id: "action-ignore", label: "Ignore Item", shortcut: "I", group: "actions" },
];

export function getCommands(): Command[] {
  return commands;
}

export function registerCommand(command: Command): void {
  commands.push(command);
}
