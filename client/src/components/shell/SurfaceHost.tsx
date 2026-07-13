/**
 * SurfaceHost — renders the active surface with transition.
 *
 * Business surfaces mount in EP-102/103/104.
 * For now renders a placeholder with the surface name.
 */

import { useStore, SURFACE_LABELS } from "../../state/store";

interface SurfaceHostProps {
  surfaces?: Record<string, React.ComponentType<Record<string, never>>>;
}

export function SurfaceHost({ surfaces = {} }: SurfaceHostProps) {
  const activeSurface = useStore((s) => s.activeSurface);
  const SurfaceComponent = surfaces[activeSurface];

  return (
    <main className="flex flex-1 flex-col overflow-auto bg-gray-950 p-4">
      {SurfaceComponent ? (
        <SurfaceComponent />
      ) : (
        <PlaceholderSurface name={SURFACE_LABELS[activeSurface]} />
      )}
    </main>
  );
}

// ── Placeholder ───────────────────────────────────────────────────────────────

function PlaceholderSurface({ name }: { name: string }) {
  return (
    <div className="flex h-full items-center justify-center">
      <p className="text-sm text-gray-600">
        <span className="font-medium text-gray-500">{name}</span> surface &mdash; mounts in
        EP-102/103/104
      </p>
    </div>
  );
}
