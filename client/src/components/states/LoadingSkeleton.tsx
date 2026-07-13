/**
 * LoadingSkeleton — animated pulse placeholder.
 *
 * Never use spinners alone per SPEC-000 visual contract.
 * Configurable rows and columns for different surface layouts.
 */

interface LoadingSkeletonProps {
  rows?: number;
  columns?: number;
  className?: string;
}

export function LoadingSkeleton({ rows = 3, columns = 1, className }: LoadingSkeletonProps) {
  return (
    <div
      className={`animate-pulse space-y-3 ${className ?? ""}`}
      role="status"
      aria-busy="true"
      aria-label="Loading content"
    >
      {Array.from({ length: rows }).map((_, rowIdx) => (
        <div key={rowIdx} className="flex gap-3">
          {Array.from({ length: columns }).map((_, colIdx) => (
            <div key={colIdx} className="h-4 flex-1 rounded bg-gray-700" />
          ))}
        </div>
      ))}
    </div>
  );
}
