// ExplainSurface: drill-down view for a single opportunity.
// Layout: Summary header -> EdgeDecomposition table -> Evidence section -> Raw JSON.

import { Opportunity } from "../../types/opportunity";
import { EdgeTable } from "../../components/edge-table";
import { StalenessChip } from "../../components/staleness-chip";

export interface ExplainSurfaceProps {
  /** The opportunity being explained. */
  opportunity: Opportunity;
  /** Venue names for display. */
  venueLabels?: { a: string; b: string };
  /** Quote staleness info. */
  staleness?: { quoteAgeMs: number; tickStaleMs: number };
  /** Confidence score (0..1) for display. */
  confidence?: string;
  /** Callback to return to the feed. */
  onBack?: () => void;
}

export function ExplainSurface({
  opportunity,
  venueLabels,
  staleness,
  confidence,
  onBack,
}: ExplainSurfaceProps) {
  const netEdge = parseFloat(opportunity.edge.net_edge);
  const conf = confidence ? parseFloat(confidence) : parseFloat(opportunity.confidence);

  return (
    <div className="space-y-6 p-4">
      {/* Back navigation */}
      {onBack && (
        <button
          onClick={onBack}
          className="text-sm text-blue-600 hover:text-blue-800 flex items-center gap-1"
        >
          &larr; Back to Feed
        </button>
      )}

      {/* Summary header */}
      <section>
        <div className="flex items-center justify-between mb-2">
          <div>
            <h2 className="text-xl font-bold text-gray-900">
              {venueLabels ? `${venueLabels.a} → ${venueLabels.b}` : "Opportunity Detail"}
            </h2>
            <p className="text-sm text-gray-500">
              {opportunity.kind} &middot; ID: {opportunity.id}
            </p>
          </div>
          {staleness && (
            <StalenessChip
              quoteAgeMs={staleness.quoteAgeMs}
              tickStaleMs={staleness.tickStaleMs}
              size="md"
            />
          )}
        </div>

        <div className="flex items-center gap-6 mt-4">
          <div>
            <div className="text-3xl font-bold text-blue-700">+{(netEdge * 100).toFixed(2)}%</div>
            <div className="text-xs text-gray-500">Net Edge</div>
          </div>
          <div>
            <div className="text-xl font-semibold text-gray-700">{(conf * 100).toFixed(0)}%</div>
            <div className="text-xs text-gray-500">Confidence</div>
          </div>
          <div>
            <div className="text-xl font-semibold text-gray-700">{opportunity.gross_edge}</div>
            <div className="text-xs text-gray-500">Gross Edge</div>
          </div>
          <div>
            <div className="text-sm font-mono text-gray-600">
              {opportunity.legs.length} leg{opportunity.legs.length !== 1 ? "s" : ""}
            </div>
            <div className="text-xs text-gray-500">Legs</div>
          </div>
        </div>
      </section>

      {/* Edge decomposition table */}
      <section>
        <h3 className="text-sm font-semibold text-gray-700 uppercase tracking-wider mb-2">
          Edge Decomposition
        </h3>
        <div className="border rounded-lg p-3 bg-white">
          <EdgeTable edge={opportunity.edge} compact />
        </div>
      </section>

      {/* Evidence section */}
      <section>
        <h3 className="text-sm font-semibold text-gray-700 uppercase tracking-wider mb-2">
          Evidence
        </h3>
        <div className="border rounded-lg p-3 bg-gray-50 space-y-2">
          {opportunity.explain_ref ? (
            <>
              <EvidenceRow label="Brain Object ID" value={opportunity.explain_ref.object_id} />
              <EvidenceRow
                label="Provenance Hash"
                value={opportunity.explain_ref.provenance_hash}
                mono
              />
            </>
          ) : (
            <p className="text-sm text-gray-400 italic">No explain reference available.</p>
          )}
          <EvidenceRow label="Trace ID" value={opportunity.trace_id ?? "N/A"} mono />
          <EvidenceRow label="Detected At" value={formatTimestamp(opportunity.detected_ts)} />
          <EvidenceRow
            label="Expires At"
            value={opportunity.expires_ts ? formatTimestamp(opportunity.expires_ts) : "Never"}
          />
          <EvidenceRow label="State" value={opportunity.state} />
        </div>
      </section>

      {/* Leg details */}
      <section>
        <h3 className="text-sm font-semibold text-gray-700 uppercase tracking-wider mb-2">Legs</h3>
        <div className="border rounded-lg overflow-hidden">
          <table className="w-full border-collapse text-sm">
            <thead>
              <tr className="bg-gray-50 text-left text-xs uppercase tracking-wider text-gray-500">
                <th className="py-2 px-3">Market</th>
                <th className="py-2 px-3">Side</th>
                <th className="py-2 px-3">Target Price</th>
                <th className="py-2 px-3">Size Hint</th>
              </tr>
            </thead>
            <tbody>
              {opportunity.legs.map((leg, i) => (
                <tr key={i} className="border-t border-gray-100">
                  <td className="py-2 px-3 font-mono text-xs text-gray-700">{leg.market}</td>
                  <td className="py-2 px-3">
                    <span
                      className={`inline-block px-2 py-0.5 rounded text-xs font-medium ${
                        leg.side === "buy"
                          ? "bg-green-100 text-green-800"
                          : "bg-red-100 text-red-800"
                      }`}
                    >
                      {leg.side}
                    </span>
                  </td>
                  <td className="py-2 px-3 font-mono text-gray-700">
                    {leg.target_price ?? "Market"}
                  </td>
                  <td className="py-2 px-3 font-mono text-gray-700">{leg.size_hint ?? "N/A"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>

      {/* Raw JSON */}
      <section>
        <details className="border rounded-lg">
          <summary className="cursor-pointer px-3 py-2 text-sm font-semibold text-gray-700 hover:bg-gray-50 rounded-lg">
            Raw JSON
          </summary>
          <pre className="p-3 text-xs font-mono bg-gray-900 text-green-400 overflow-x-auto rounded-b-lg">
            {JSON.stringify(opportunity, null, 2)}
          </pre>
        </details>
      </section>
    </div>
  );
}

function EvidenceRow({
  label,
  value,
  mono = false,
}: {
  label: string;
  value: string;
  mono?: boolean;
}) {
  return (
    <div className="flex items-center justify-between">
      <span className="text-xs text-gray-500">{label}</span>
      <span className={`text-sm ${mono ? "font-mono text-gray-700" : "text-gray-800"}`}>
        {value}
      </span>
    </div>
  );
}

function formatTimestamp(ts: string): string {
  try {
    const d = new Date(ts);
    return d.toLocaleString();
  } catch {
    return ts;
  }
}
