// useFeedState hook: stub for WebSocket-backed feed state.
// Replace with actual WS subscription when the backend surfaces layer is wired.

import React from "react";
import { createFeedState, FeedState } from "../../state/feed";

export interface FeedActions {
  explain: (id: string) => void;
  act: (id: string) => void;
  ignore: (id: string) => void;
  simulate: (id: string) => void;
}

export function useFeedState(): { state: FeedState; actions: FeedActions } {
  const [state] = React.useState<FeedState>(createFeedState);

  const actions: FeedActions = {
    explain: (id: string) => console.log("explain", id),
    act: (id: string) => console.log("act", id),
    ignore: (id: string) => console.log("ignore", id),
    simulate: (id: string) => console.log("simulate", id),
  };

  return { state, actions };
}
