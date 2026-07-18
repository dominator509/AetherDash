// Feed state: normalized by opportunity ID.
// Updates coalesce on lifecycle transitions (no duplicate cards).

import { FeedItem } from "../types/opportunity";

export interface FeedState {
  /** All feed items, keyed by opportunity ID. */
  items: Map<string, FeedItem>;
  /** Sorted IDs for display order (newest first by default). */
  order: string[];
  /** Whether the feed is in degraded mode. */
  degraded: boolean;
  /** Last update timestamp. */
  lastUpdate: number;
}

/** Create an empty feed state. */
export function createFeedState(): FeedState {
  return { items: new Map(), order: [], degraded: false, lastUpdate: 0 };
}

/** Insert or update a feed item. Lifecycle updates coalesce. */
export function upsertFeedItem(state: FeedState, item: FeedItem): FeedState {
  const id = item.opportunity.id;
  if (!state.items.has(id)) {
    // New item: insert at front
    state.order.unshift(id);
  }
  state.items.set(id, item);
  state.lastUpdate = Date.now();
  return state;
}

/** Remove expired items from the feed. */
export function expireFeedItems(state: FeedState, maxAgeMs: number): FeedState {
  const now = Date.now();
  for (const [id, item] of state.items) {
    if (item.opportunity.state === "expired" || item.opportunity.state === "closed") {
      const detectedAt = new Date(item.opportunity.detected_ts).getTime();
      if (now - detectedAt > maxAgeMs) {
        state.items.delete(id);
        state.order = state.order.filter((o) => o !== id);
      }
    }
  }
  return state;
}

/** Set degradation mode. */
export function setFeedDegraded(state: FeedState, degraded: boolean): FeedState {
  state.degraded = degraded;
  return state;
}

/** Get feed items sorted by current order. */
export function getOrderedItems(state: FeedState): FeedItem[] {
  return state.order.map((id) => state.items.get(id)!).filter(Boolean);
}
