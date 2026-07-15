// Staleness chip: shows quote age and colors based on venue tick_stale_ms.
// SPEC-004: staleness is NOT color-only — text label always present.

export interface StalenessChipProps {
  quoteAgeMs: number;
  tickStaleMs: number;
  size?: 'sm' | 'md';
}

export function StalenessChip({ quoteAgeMs, tickStaleMs, size = 'md' }: StalenessChipProps) {
  const ratio = quoteAgeMs / tickStaleMs;
  let variant: 'fresh' | 'aging' | 'stale' = 'fresh';
  if (ratio > 2) variant = 'stale';
  else if (ratio > 1) variant = 'aging';

  const colors = {
    fresh: 'bg-green-100 text-green-800',
    aging: 'bg-yellow-100 text-yellow-800',
    stale: 'bg-red-100 text-red-800',
  };

  const labels = {
    fresh: 'Fresh',
    aging: 'Aging',
    stale: 'Stale',
  };

  return (
    <span
      className={`inline-flex items-center rounded-full px-2 py-0.5 text-${size === 'sm' ? 'xs' : 'sm'} font-medium ${colors[variant]}`}
      aria-label={`Quote: ${labels[variant]} (${quoteAgeMs}ms / ${tickStaleMs}ms limit)`}
    >
      {labels[variant]} {quoteAgeMs}ms
    </span>
  );
}
