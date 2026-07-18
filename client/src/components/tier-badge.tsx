// Tier badge: always visible, reflects server-provided session tier.
// SPEC-005: tier is server-decided, never client-config.

const DEFAULT_TIER = { label: "Read-Only", color: "bg-gray-100 text-gray-700" };

const TIER_LABELS: Record<number, { label: string; color: string }> = {
  1: DEFAULT_TIER,
  2: { label: "Draft-Only", color: "bg-blue-100 text-blue-700" },
  3: { label: "Confirm-Every", color: "bg-yellow-100 text-yellow-800" },
  4: { label: "Bounded-Auto", color: "bg-orange-100 text-orange-800" },
  5: { label: "YOLO", color: "bg-purple-100 text-purple-800" },
};

export interface TierBadgeProps {
  tier: number;
  size?: "sm" | "md";
}

export function TierBadge({ tier, size = "md" }: TierBadgeProps) {
  const config = TIER_LABELS[tier] ?? DEFAULT_TIER;
  return (
    <span
      className={`inline-flex items-center rounded-full font-bold ${size === "sm" ? "px-2 py-0 text-xs" : "px-3 py-1 text-sm"} ${config.color}`}
      title={`Authorization tier ${tier}: ${config.label}`}
    >
      T{tier}: {config.label}
    </span>
  );
}
