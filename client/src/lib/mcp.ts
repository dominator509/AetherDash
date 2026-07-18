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
): Promise<SwarmLaunchResult> {
  const body = confirmationRef ? { ...request, confirmation_ref: confirmationRef } : request;
  const resp = await fetch(`${gatewayUrl}/mcp/tools/swarm.launch`, {
    method: "POST",
    headers: { Authorization: `Bearer ${sessionToken}`, "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  const data = await resp.json();
  if (resp.status === 412 && typeof data.details === "string") {
    const prefix = "confirmation_ref=";
    if (data.details.startsWith(prefix)) {
      return {
        status: "confirmation_required",
        confirmationRef: data.details.slice(prefix.length),
      };
    }
  }
  if (!resp.ok) throw new Error(data.message ?? `Swarm launch failed: ${resp.status}`);
  if (data.result?.proposal_only !== true) {
    throw new Error("Swarm result violated the proposal-only contract");
  }
  return { status: "completed", packet: data.result, progress: data.progress ?? [] };
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
