/**
 * DegradationBanner — renders from SPEC-003 degradation frame.
 *
 * Names the affected surface + reason.
 * Persistent (not auto-dismissing).
 * Muted amber styling (not red — degradation is not an error per SPEC-000).
 */

interface DegradationBannerProps {
  surface: string;
  reason: string;
  started_at: string;
}

export function DegradationBanner({ surface, reason, started_at }: DegradationBannerProps) {
  return (
    <div
      className="flex items-center gap-2 rounded border border-amber-800/30 bg-amber-900/30 px-3 py-1.5 text-sm"
      role="status"
      aria-live="polite"
    >
      <span className="font-medium capitalize text-amber-400">{surface}</span>
      <span className="text-amber-300/70">&mdash;</span>
      <span className="text-amber-300/80">{reason}</span>
      <span className="ml-auto text-xs text-amber-400/60">{started_at}</span>
    </div>
  );
}
