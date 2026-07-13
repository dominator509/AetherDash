/**
 * CommandPalette — overlay modal triggered by Ctrl/Cmd+K.
 *
 * Radix Dialog with search input, fuzzy-filtered command list,
 * keyboard navigation (arrows + Enter), and animated open/close.
 */

import { useState, useEffect, useRef, useCallback, useMemo } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import Fuse from "fuse.js";
import { useStore } from "../../state/store";
import type { SurfaceName } from "../../state/store";
import { getCommands } from "./commandRegistry";
import type { Command } from "./commandRegistry";

// ── Surface icon display ───────────────────────────────────────────────────

function surfaceSymbol(surface?: SurfaceName): string {
  if (!surface) return "⚙";
  const symbols: Partial<Record<SurfaceName, string>> = {
    feed: "≡",
    explain: "ⓘ",
    simulate: "▶",
    ticket: "⊞",
    command: "⌘",
    alerts: "⚡",
    positions: "📊",
    settings: "⚙",
  };
  return symbols[surface] ?? "⚙";
}

// ── Group labels ───────────────────────────────────────────────────────────

const GROUP_LABELS: Record<string, string> = {
  navigation: "Navigation",
  actions: "Actions",
  settings: "Settings",
};

// ── Component ──────────────────────────────────────────────────────────────

export function CommandPalette() {
  const paletteOpen = useStore((s) => s.paletteOpen);
  const closePalette = useStore((s) => s.closePalette);
  const setActiveSurface = useStore((s) => s.setActiveSurface);

  const [query, setQuery] = useState("");
  const [selectedIdx, setSelectedIdx] = useState(0);
  const searchRef = useRef<HTMLInputElement>(null);

  // Get commands from the registry
  const commands = useMemo(() => getCommands(), []);

  // Fuse.js fuzzy search instance
  const fuse = useMemo(
    () =>
      new Fuse(commands, {
        keys: ["label", "id"],
        threshold: 0.4,
        includeScore: true,
      }),
    [commands],
  );

  // Filtered results from query
  const results = useMemo(() => {
    if (!query.trim()) return commands;
    return fuse.search(query).map((r) => r.item);
  }, [query, commands, fuse]);

  // Reset state when opening
  useEffect(() => {
    if (paletteOpen) {
      setQuery("");
      setSelectedIdx(0);
      // Auto-focus search input after dialog renders
      requestAnimationFrame(() => {
        searchRef.current?.focus();
      });
    }
  }, [paletteOpen]);

  // Execute a command: navigate to surface or run custom action
  const execute = useCallback(
    (cmd: Command) => {
      if (cmd.surface) {
        setActiveSurface(cmd.surface);
      }
      cmd.action?.();
      closePalette();
    },
    [setActiveSurface, closePalette],
  );

  // Keyboard navigation within palette
  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setSelectedIdx((prev) => Math.min(prev + 1, results.length - 1));
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        setSelectedIdx((prev) => Math.max(prev - 1, 0));
      } else if (e.key === "Enter") {
        e.preventDefault();
        const cmd = results[selectedIdx];
        if (cmd) execute(cmd);
      }
      // Escape is handled by Radix Dialog natively
    },
    [results, selectedIdx, execute],
  );

  // Group results by command group
  const groupedResults = useMemo(() => {
    const groups = new Map<string, Command[]>();
    for (const cmd of results) {
      const existing = groups.get(cmd.group) ?? [];
      existing.push(cmd);
      groups.set(cmd.group, existing);
    }
    return Array.from(groups.entries());
  }, [results]);

  return (
    <Dialog.Root
      open={paletteOpen}
      onOpenChange={(open) => {
        if (!open) closePalette();
      }}
    >
      <Dialog.Portal>
        {/* Overlay backdrop */}
        <Dialog.Overlay className="fixed inset-0 bg-black/60 backdrop-blur-sm transition-opacity duration-200 data-[state=closed]:opacity-0 data-[state=open]:opacity-100" />

        {/* Content panel */}
        <Dialog.Content
          onKeyDown={handleKeyDown}
          className="fixed left-1/2 top-1/4 w-full max-w-lg -translate-x-1/2 -translate-y-1/4 rounded-lg border border-gray-700 bg-gray-900 shadow-2xl transition-all duration-200 data-[state=closed]:scale-95 data-[state=closed]:opacity-0 data-[state=open]:scale-100 data-[state=open]:opacity-100"
        >
          {/* Search input */}
          <div className="border-b border-gray-700 px-4 py-3">
            <input
              ref={searchRef}
              type="text"
              value={query}
              onChange={(e) => {
                setQuery(e.target.value);
                setSelectedIdx(0);
              }}
              placeholder="Search commands and surfaces..."
              className="w-full bg-transparent text-sm text-gray-100 placeholder-gray-500 outline-none"
              aria-label="Search commands"
            />
          </div>

          {/* Results list */}
          {results.length === 0 ? (
            <div className="px-4 py-8 text-center text-sm text-gray-500">No commands found</div>
          ) : (
            <div className="max-h-80 overflow-y-auto py-2">
              {groupedResults.map(([group, cmds]) => (
                <div key={group}>
                  <div className="px-4 py-1.5 text-xs font-medium uppercase tracking-wider text-gray-500">
                    {GROUP_LABELS[group] ?? group}
                  </div>
                  {cmds.map((cmd) => {
                    const globalIdx = results.indexOf(cmd);
                    const isSelected = globalIdx === selectedIdx;
                    return (
                      <button
                        key={cmd.id}
                        onClick={() => execute(cmd)}
                        onMouseEnter={() => setSelectedIdx(globalIdx)}
                        className={`flex w-full items-center gap-3 px-4 py-2 text-left text-sm transition-colors ${
                          isSelected
                            ? "bg-blue-600/20 text-blue-300"
                            : "text-gray-300 hover:bg-gray-800"
                        }`}
                        role="option"
                        aria-selected={isSelected}
                      >
                        <span className="flex h-6 w-6 items-center justify-center rounded bg-gray-800 text-xs">
                          {surfaceSymbol(cmd.surface)}
                        </span>
                        <span className="flex-1">{cmd.label}</span>
                        {cmd.shortcut && (
                          <span className="font-mono text-xs text-gray-500">{cmd.shortcut}</span>
                        )}
                      </button>
                    );
                  })}
                </div>
              ))}
            </div>
          )}

          {/* Footer hint */}
          <div className="border-t border-gray-700 px-4 py-2 text-xs text-gray-600">
            <span className="font-mono">&uarr;&darr;</span> Navigate &nbsp;
            <span className="font-mono">&crarr;</span> Select &nbsp;
            <span className="font-mono">Esc</span> Close
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
