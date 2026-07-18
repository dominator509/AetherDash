// EdgeDecomposition table: renders all 11 components including explicit zeros.
// SPEC-012: "never a bare profit number" — decomposition always shown.

import { EdgeDecomposition } from "../types/opportunity";

export interface EdgeTableProps {
  edge: EdgeDecomposition;
  compact?: boolean;
}

const COMPONENT_LABELS: [keyof EdgeDecomposition, string][] = [
  ["gross_spread", "Gross Spread"],
  ["fees", "Fees"],
  ["slippage_est", "Slippage (est)"],
  ["funding_cost", "Funding Cost"],
  ["gas_cost", "Gas Cost"],
  ["bridge_cost", "Bridge Cost"],
  ["settlement_mismatch_discount", "Settlement Mismatch"],
  ["liquidity_haircut", "Liquidity Haircut"],
  ["staleness_penalty", "Staleness Penalty"],
  ["confidence_penalty", "Confidence Penalty"],
  ["net_edge", "NET EDGE"],
];

export function EdgeTable({ edge, compact = false }: EdgeTableProps) {
  const rows = COMPONENT_LABELS.map(([key, label]) => ({
    label,
    value: edge[key],
    isNet: key === "net_edge",
    isZero: edge[key] === "0" || edge[key] === "0.00",
  }));

  return (
    <div className={compact ? "text-sm" : "text-base"}>
      <table className="w-full border-collapse">
        <tbody>
          {rows.map((row) => (
            <tr
              key={row.label}
              className={`border-b border-gray-100 ${row.isNet ? "font-bold bg-gray-50" : ""}`}
            >
              <td className="py-1 px-2 text-gray-600">{row.label}</td>
              <td
                className={`py-1 px-2 text-right font-mono ${row.isNet ? "text-blue-700" : row.isZero ? "text-gray-400 italic" : "text-gray-900"}`}
              >
                {row.isZero ? "0.00 (not applicable)" : row.value}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
