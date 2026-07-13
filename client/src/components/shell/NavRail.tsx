/**
 * NavRail — left navigation rail (48px wide).
 *
 * Icon buttons for each surface with active highlighting.
 * Tooltips on hover. Keyboard navigable with arrow keys.
 */

import { useCallback } from "react";
import * as Tooltip from "@radix-ui/react-tooltip";
import type { SurfaceName } from "../../state/store";
import { SURFACE_LABELS, useStore } from "../../state/store";

// ── Simple SVG icon components ────────────────────────────────────────────────

function FeedIcon({ className }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 20 20" fill="currentColor" aria-hidden="true">
      <path d="M3 4a1 1 0 011-1h12a1 1 0 110 2H4a1 1 0 01-1-1zm0 4a1 1 0 011-1h12a1 1 0 110 2H4a1 1 0 01-1-1zm0 4a1 1 0 011-1h12a1 1 0 110 2H4a1 1 0 01-1-1zm0 4a1 1 0 011-1h12a1 1 0 110 2H4a1 1 0 01-1-1z" />
    </svg>
  );
}

function ExplainIcon({ className }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 20 20" fill="currentColor" aria-hidden="true">
      <path
        fillRule="evenodd"
        d="M18 10a8 8 0 11-16 0 8 8 0 0116 0zm-7-4a1 1 0 11-2 0 1 1 0 012 0zM9 9a1 1 0 000 2v3a1 1 0 001 1h1a1 1 0 100-2v-3a1 1 0 00-1-1H9z"
        clipRule="evenodd"
      />
    </svg>
  );
}

function SimulateIcon({ className }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 20 20" fill="currentColor" aria-hidden="true">
      <path
        fillRule="evenodd"
        d="M10 18a8 8 0 100-16 8 8 0 000 16zM9.555 7.168A1 1 0 008 8v4a1 1 0 001.555.832l3-2a1 1 0 000-1.664l-3-2z"
        clipRule="evenodd"
      />
    </svg>
  );
}

function TicketIcon({ className }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 20 20" fill="currentColor" aria-hidden="true">
      <path d="M2 6a2 2 0 012-2h12a2 2 0 012 2v2a2 2 0 100 4v2a2 2 0 01-2 2H4a2 2 0 01-2-2v-2a2 2 0 100-4V6z" />
    </svg>
  );
}

function CommandIcon({ className }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 20 20" fill="currentColor" aria-hidden="true">
      <path
        fillRule="evenodd"
        d="M3 5a2 2 0 012-2h10a2 2 0 012 2v10a2 2 0 01-2 2H5a2 2 0 01-2-2V5zm3.293 2.293a1 1 0 011.414 0l3 3a1 1 0 010 1.414l-3 3a1 1 0 01-1.414-1.414L8.586 10 6.293 7.707a1 1 0 010-1.414zM11 13a1 1 0 100 2h3a1 1 0 100-2h-3z"
        clipRule="evenodd"
      />
    </svg>
  );
}

function AlertsIcon({ className }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 20 20" fill="currentColor" aria-hidden="true">
      <path d="M10 2a6 6 0 00-6 6v3.586l-.707.707A1 1 0 004 14h12a1 1 0 00.707-1.707L16 11.586V8a6 6 0 00-6-6zM10 18a3 3 0 01-3-3h6a3 3 0 01-3 3z" />
    </svg>
  );
}

function PositionsIcon({ className }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 20 20" fill="currentColor" aria-hidden="true">
      <path d="M4 4a2 2 0 00-2 2v1h16V6a2 2 0 00-2-2H4z" />
      <path
        fillRule="evenodd"
        d="M18 9H2v5a2 2 0 002 2h12a2 2 0 002-2V9zM4 13a1 1 0 011-1h1a1 1 0 110 2H5a1 1 0 01-1-1zm5-1a1 1 0 100 2h1a1 1 0 100-2H9z"
        clipRule="evenodd"
      />
    </svg>
  );
}

function SettingsIcon({ className }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 20 20" fill="currentColor" aria-hidden="true">
      <path
        fillRule="evenodd"
        d="M11.49 3.17c-.38-1.56-2.6-1.56-2.98 0a1.532 1.532 0 01-2.286.948c-1.372-.836-2.942.734-2.106 2.106.54.886.061 2.042-.947 2.287-1.561.379-1.561 2.6 0 2.978a1.532 1.532 0 01.947 2.287c-.836 1.372.734 2.942 2.106 2.106a1.532 1.532 0 012.287.947c.379 1.561 2.6 1.561 2.978 0a1.532 1.532 0 012.287-.947c1.372.836 2.942-.734 2.106-2.106a1.532 1.532 0 01.947-2.287c1.561-.379 1.561-2.6 0-2.978a1.532 1.532 0 01-.947-2.287c.836-1.372-.734-2.942-2.106-2.106a1.532 1.532 0 01-2.287-.947zM10 13a3 3 0 100-6 3 3 0 000 6z"
        clipRule="evenodd"
      />
    </svg>
  );
}

// ── Surface icon map ──────────────────────────────────────────────────────────

const SURFACE_ICONS: Record<SurfaceName, typeof FeedIcon> = {
  feed: FeedIcon,
  explain: ExplainIcon,
  simulate: SimulateIcon,
  ticket: TicketIcon,
  command: CommandIcon,
  alerts: AlertsIcon,
  positions: PositionsIcon,
  settings: SettingsIcon,
};

// ── NavRail component ─────────────────────────────────────────────────────────

export function NavRail() {
  const activeSurface = useStore((s) => s.activeSurface);
  const setActiveSurface = useStore((s) => s.setActiveSurface);

  const surfaces = Object.keys(SURFACE_LABELS) as SurfaceName[];

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent, _surface: SurfaceName) => {
      const currentIdx = surfaces.indexOf(activeSurface);
      let nextIdx = currentIdx;

      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        nextIdx =
          e.key === "ArrowDown"
            ? (currentIdx + 1) % surfaces.length
            : (currentIdx - 1 + surfaces.length) % surfaces.length;
        setActiveSurface(surfaces[nextIdx] as SurfaceName);
      }
    },
    [activeSurface, surfaces, setActiveSurface],
  );

  return (
    <nav
      className="flex w-12 flex-col items-center gap-1 border-r border-gray-800 bg-gray-950 py-2"
      aria-label="Surface navigation"
      role="navigation"
    >
      {surfaces.map((surface) => {
        const Icon = SURFACE_ICONS[surface];
        const isActive = surface === activeSurface;
        const label = SURFACE_LABELS[surface];

        return (
          <Tooltip.Root key={surface}>
            <Tooltip.Trigger asChild>
              <button
                onClick={() => setActiveSurface(surface)}
                onKeyDown={(e) => handleKeyDown(e, surface)}
                className={`flex h-9 w-9 items-center justify-center rounded-md transition-colors focus:outline-none focus:ring-2 focus:ring-blue-500 ${
                  isActive
                    ? "bg-blue-600/20 text-blue-400"
                    : "text-gray-500 hover:bg-gray-800 hover:text-gray-300"
                }`}
                aria-label={label}
                aria-current={isActive ? "page" : undefined}
                tabIndex={0}
              >
                <Icon className="h-5 w-5" />
              </button>
            </Tooltip.Trigger>
            <Tooltip.Portal>
              <Tooltip.Content
                side="right"
                sideOffset={6}
                className="rounded-md bg-gray-800 px-2.5 py-1 text-xs text-gray-200 shadow-lg"
              >
                {label}
                <Tooltip.Arrow className="fill-gray-800" />
              </Tooltip.Content>
            </Tooltip.Portal>
          </Tooltip.Root>
        );
      })}
    </nav>
  );
}
