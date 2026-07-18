// Simple card feed surface (SPEC-004 Surface 1).
// Renders feed items as cards with actions: Explain, Simulate, Act, Ignore.

import { useMemo } from "react";
import { FeedItem } from "../../types/opportunity";
import { getOrderedItems } from "../../state/feed";
import { StalenessChip } from "../../components/staleness-chip";
import { useFeedState } from "./useFeedState";

export interface FeedSurfaceProps {
  /** Whether to show the advanced table (Surface 2) instead of cards (Surface 1). */
  mode?: "simple" | "advanced";
}

export function FeedSurface({ mode = "simple" }: FeedSurfaceProps) {
  const { state, actions } = useFeedState();
  const items = useMemo(() => getOrderedItems(state), [state]);

  if (state.degraded) {
    return (
      <div className="p-4 bg-yellow-50 border border-yellow-200 rounded" role="alert">
        <p className="text-yellow-800 font-medium">Feed Degraded</p>
        <p className="text-yellow-600 text-sm">Market data may be delayed or incomplete.</p>
      </div>
    );
  }

  if (items.length === 0) {
    return (
      <div className="p-8 text-center text-gray-400">
        <p>No opportunities detected.</p>
        <p className="text-sm">
          Opportunities will appear here when the scanner detects cross-venue edges.
        </p>
      </div>
    );
  }

  if (mode === "advanced") {
    return (
      <div className="p-4">
        <table className="w-full border-collapse">
          <thead>
            <tr className="border-b-2 border-gray-200 text-left text-xs uppercase tracking-wider text-gray-500">
              <th className="py-2 px-3">Asset</th>
              <th className="py-2 px-3">Venues</th>
              <th className="py-2 px-3">Net Edge</th>
              <th className="py-2 px-3">Confidence</th>
              <th className="py-2 px-3">Staleness</th>
              <th className="py-2 px-3">Kind</th>
              <th className="py-2 px-3">Actions</th>
            </tr>
          </thead>
          <tbody>
            {items.map((item) => (
              <AdvancedRow
                key={item.opportunity.id}
                item={item}
                onExplain={() => actions.explain(item.opportunity.id)}
                onAct={() => actions.act(item.opportunity.id)}
              />
            ))}
          </tbody>
        </table>
      </div>
    );
  }

  return (
    <div className="space-y-3 p-4">
      {items.map((item) => (
        <OpportunityCard
          key={item.opportunity.id}
          item={item}
          onExplain={() => actions.explain(item.opportunity.id)}
          onAct={() => actions.act(item.opportunity.id)}
          onIgnore={() => actions.ignore(item.opportunity.id)}
        />
      ))}
    </div>
  );
}

function AdvancedRow({
  item,
  onExplain,
  onAct,
}: {
  item: FeedItem;
  onExplain: () => void;
  onAct: () => void;
}) {
  const { opportunity, hints } = item;
  const netEdge = parseFloat(opportunity.edge.net_edge);
  const confidence = parseFloat(opportunity.confidence);

  return (
    <tr className="border-b border-gray-100 hover:bg-gray-50">
      <td className="py-2 px-3 font-medium text-gray-900">{hints.asset_label}</td>
      <td className="py-2 px-3 text-gray-600">
        {hints.venue_a} &rarr; {hints.venue_b}
      </td>
      <td className="py-2 px-3 font-mono text-blue-700">+{(netEdge * 100).toFixed(2)}%</td>
      <td className="py-2 px-3 text-gray-600">{(confidence * 100).toFixed(0)}%</td>
      <td className="py-2 px-3">
        <StalenessChip
          quoteAgeMs={hints.quote_age_ms}
          tickStaleMs={hints.tick_stale_ms}
          size="sm"
        />
      </td>
      <td className="py-2 px-3 text-xs text-gray-500">{opportunity.kind}</td>
      <td className="py-2 px-3">
        <div className="flex gap-1">
          <button
            onClick={onExplain}
            className="px-2 py-1 text-xs bg-gray-100 hover:bg-gray-200 rounded"
          >
            Explain
          </button>
          <button
            onClick={onAct}
            className="px-2 py-1 text-xs bg-blue-600 text-white hover:bg-blue-700 rounded"
          >
            Act
          </button>
        </div>
      </td>
    </tr>
  );
}

function OpportunityCard({
  item,
  onExplain,
  onAct,
  onIgnore,
}: {
  item: FeedItem;
  onExplain: () => void;
  onAct: () => void;
  onIgnore: () => void;
}) {
  const { opportunity, hints } = item;
  const netEdge = parseFloat(opportunity.edge.net_edge);
  const confidence = parseFloat(opportunity.confidence);

  return (
    <div className="border rounded-lg p-4 shadow-sm hover:shadow-md transition-shadow">
      <div className="flex items-center justify-between mb-2">
        <h3 className="font-semibold text-gray-900">
          {hints.asset_label}: {hints.venue_a} → {hints.venue_b}
        </h3>
        <div className="flex items-center gap-2">
          <StalenessChip
            quoteAgeMs={hints.quote_age_ms}
            tickStaleMs={hints.tick_stale_ms}
            size="sm"
          />
          <span className="text-xs text-gray-500">{opportunity.kind}</span>
        </div>
      </div>

      <div className="flex items-center gap-4 mb-3">
        <div className="text-2xl font-bold text-blue-700">+{(netEdge * 100).toFixed(2)}%</div>
        <div className="text-sm text-gray-500">confidence: {(confidence * 100).toFixed(0)}%</div>
      </div>

      <div className="flex gap-2">
        <button
          onClick={onExplain}
          className="px-3 py-1 text-sm bg-gray-100 hover:bg-gray-200 rounded"
        >
          Explain
        </button>
        <button
          onClick={onAct}
          className="px-3 py-1 text-sm bg-blue-600 text-white hover:bg-blue-700 rounded"
        >
          Act
        </button>
        <button
          onClick={onIgnore}
          className="px-3 py-1 text-sm text-gray-500 hover:text-gray-700 rounded"
        >
          Ignore
        </button>
      </div>
    </div>
  );
}

// Re-export from state module for caller convenience
export { createFeedState } from "../../state/feed";
