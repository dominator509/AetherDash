// MCP (Model Context Protocol) client over the gateway transport.
// Fetches tier-filtered tool inventory. Renders only received tools.
// INV-1: client reflects server, never assumes unlisted tools.

export interface McpTool {
  name: string;
  description: string;
  inputSchema: Record<string, unknown>;
}

export interface McpToolInventory {
  tools: McpTool[];
  serverTier: number;
  fetchedAt: number;
}

export interface SwarmBudget {
  max_calls: number;
  max_tokens: number;
  max_cost_usd: string;
  max_seconds: number;
}

export interface SwarmLaunchRequest {
  question: string;
  budget: SwarmBudget;
  context?: Record<string, unknown>;
  workers?: number;
}

export interface BrainCitation {
  object_id: string;
  provenance_hash: string;
}

export interface DecisionClaim {
  text: string;
  citations: BrainCitation[];
}

export interface DecisionPacket {
  recommendation: DecisionClaim;
  confidence: number;
  rationale: DecisionClaim[];
  citations: BrainCitation[];
  budget_used: {
    calls: number;
    tokens: number;
    cost_usd: string;
    elapsed_seconds: number;
  };
  budget_truncated: boolean;
  truncated_dimension?: string | null;
  proposal_only: true;
}

export interface SwarmProgressEvent {
  kind: string;
  worker_id?: string | null;
  detail?: string | null;
}

export type SwarmLaunchResult =
  | { status: "confirmation_required"; confirmationRef: string }
  | { status: "completed"; packet: DecisionPacket; progress: Array<Record<string, unknown>> };

/** Fetch the tier-filtered tool inventory from the gateway. */
export async function fetchToolInventory(
  gatewayUrl: string,
  sessionToken: string,
): Promise<McpToolInventory> {
  const resp = await fetch(`${gatewayUrl}/mcp/tools`, {
    headers: { Authorization: `Bearer ${sessionToken}`, "Content-Type": "application/json" },
  });
  if (!resp.ok) throw new Error(`MCP inventory fetch failed: ${resp.status}`);
  const data = await resp.json();
  return { tools: data.tools ?? [], serverTier: data.tier ?? 1, fetchedAt: Date.now() };
}

/** Launch or confirm a bounded swarm through the tier-filtered MCP gateway. */
export async function launchSwarm(
  gatewayUrl: string,
  sessionToken: string,
  request: SwarmLaunchRequest,
  confirmationRef?: string,
  onProgress?: (event: SwarmProgressEvent) => void,
): Promise<SwarmLaunchResult> {
  const body = confirmationRef ? { ...request, confirmation_ref: confirmationRef } : request;
  const resp = await fetch(`${gatewayUrl}/mcp/tools/swarm.launch/stream`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${sessionToken}`,
      "Content-Type": "application/json",
      Accept: "application/x-ndjson",
    },
    body: JSON.stringify(body),
  });
  if (!resp.ok) {
    const data = await resp.json();
    const prefix = "confirmation_ref=";
    if (
      resp.status === 412 &&
      typeof data.details === "string" &&
      data.details.startsWith(prefix)
    ) {
      return {
        status: "confirmation_required",
        confirmationRef: data.details.slice(prefix.length),
      };
    }
    throw new Error(data.message ?? `Swarm launch failed: ${resp.status}`);
  }

  if (!resp.body) throw new Error("Swarm stream was unavailable");
  const progress: Array<Record<string, unknown>> = [];
  let packet: DecisionPacket | undefined;
  const decoder = new TextDecoder();
  const reader = resp.body.getReader();
  let buffered = "";

  const consume = (line: string) => {
    if (!line.trim()) return;
    const record = JSON.parse(line) as {
      type?: string;
      event?: SwarmProgressEvent;
      result?: DecisionPacket;
      error?: { message?: string };
    };
    if (record.type === "progress" && record.event) {
      progress.push(record.event as unknown as Record<string, unknown>);
      onProgress?.(record.event);
      return;
    }
    if (record.type === "packet" && record.result) {
      if (packet) throw new Error("Swarm stream emitted more than one decision packet");
      packet = record.result;
      return;
    }
    if (record.type === "error") {
      throw new Error(record.error?.message ?? "Swarm failed before producing a packet");
    }
    throw new Error("Swarm stream emitted an invalid record");
  };

  while (true) {
    const { done, value } = await reader.read();
    buffered += decoder.decode(value, { stream: !done });
    const lines = buffered.split("\n");
    buffered = lines.pop() ?? "";
    lines.forEach(consume);
    if (done) break;
  }
  consume(buffered);
  if (!packet || packet.proposal_only !== true) {
    throw new Error("Swarm result violated the exactly-one proposal packet contract");
  }
  return { status: "completed", packet, progress };
}

/** Stable command-room text projection; citations remain visible and copyable. */
export function formatDecisionPacket(packet: DecisionPacket): string {
  const rationale = packet.rationale
    .map(
      (claim) =>
        `- ${claim.text} [${claim.citations.map((citation) => citation.object_id).join(", ")}]`,
    )
    .join("\n");
  const truncation = packet.budget_truncated
    ? `\nBudget truncated: ${packet.truncated_dimension ?? "unknown"}`
    : "";
  return `Recommendation: ${packet.recommendation.text}\nConfidence: ${packet.confidence.toFixed(2)}\nRationale:\n${rationale}${truncation}\nProposal only — no action was executed.`;
}

/** Check if a command matches an available tool. */
export function findTool(inventory: McpToolInventory, command: string): McpTool | undefined {
  return inventory.tools.find((t) => t.name === command || t.name === command.split(" ")[0]);
}

/** Available slash commands derived from the tool inventory. */
export function getSlashCommands(
  inventory: McpToolInventory,
): Array<{ command: string; description: string }> {
  return inventory.tools.map((t) => ({ command: `/${t.name}`, description: t.description }));
}
