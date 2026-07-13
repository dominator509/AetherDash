/**
 * StatusBar — top status bar (32px).
 *
 * Connection indicator, tier badge, degradation banner slot,
 * mode toggle, and clock/session status.
 */

import { useEffect, useState } from "react";
import { useStore } from "../../state/store";
import { DegradationBanner } from "../states/DegradationBanner";
import { ModeToggle } from "../toggle/ModeToggle";

// ── Connection indicator ──────────────────────────────────────────────────────

const CONNECTION_CONFIG: Record<string, { color: string; label: string }> = {
  connected: { color: "bg-green-500", label: "Connected" },
  connecting: { color: "bg-yellow-500", label: "Connecting" },
  reconnecting: { color: "bg-yellow-500", label: "Reconnecting" },
  disconnected: { color: "bg-red-500", label: "Disconnected" },
};

// ── StatusBar component ───────────────────────────────────────────────────────

export function StatusBar() {
  const connectionStatus = useStore((s) => s.connectionStatus);
  const tier = useStore((s) => s.tier);
  const degradations = useStore((s) => s.degradations);

  const [clock, setClock] = useState("");

  useEffect(() => {
    const update = () => {
      const now = new Date();
      setClock(
        now.toLocaleTimeString(undefined, {
          hour: "2-digit",
          minute: "2-digit",
          second: "2-digit",
          hour12: false,
        }),
      );
    };
    update();
    const id = setInterval(update, 1000);
    return () => clearInterval(id);
  }, []);

  const DISCONNECTED_ENTRY = { color: "bg-red-500", label: "Disconnected" };
  const conn = CONNECTION_CONFIG[connectionStatus] ?? DISCONNECTED_ENTRY;

  return (
    <header className="flex h-8 shrink-0 items-center gap-3 border-b border-gray-800 bg-gray-950 px-3 text-xs">
      {/* Connection indicator */}
      <div className="flex items-center gap-1.5">
        <span className={`h-2 w-2 rounded-full ${conn.color}`} />
        <span className="text-gray-400">{conn.label}</span>
      </div>

      {/* Tier badge */}
      {tier !== null && (
        <span className="rounded bg-gray-800 px-1.5 py-0.5 font-mono text-xs text-gray-400">
          T{tier}
        </span>
      )}

      {/* Degradation banner slot */}
      {degradations.length > 0 && (
        <div className="flex items-center gap-2 overflow-hidden" role="status" aria-live="polite">
          {degradations.map((d) => (
            <DegradationBanner
              key={d.surface}
              surface={d.surface}
              reason={d.reason}
              started_at={d.started_at}
            />
          ))}
        </div>
      )}

      {/* Spacer */}
      <div className="flex-1" />

      {/* Mode toggle */}
      <ModeToggle />

      {/* Clock */}
      <time className="font-mono text-gray-500" dateTime={clock}>
        {clock}
      </time>
    </header>
  );
}
