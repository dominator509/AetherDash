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

/** Fetch the tier-filtered tool inventory from the gateway. */
export async function fetchToolInventory(gatewayUrl: string, sessionToken: string): Promise<McpToolInventory> {
  const resp = await fetch(`${gatewayUrl}/mcp/tools`, {
    headers: { Authorization: `Bearer ${sessionToken}`, 'Content-Type': 'application/json' },
  });
  if (!resp.ok) throw new Error(`MCP inventory fetch failed: ${resp.status}`);
  const data = await resp.json();
  return { tools: data.tools ?? [], serverTier: data.tier ?? 1, fetchedAt: Date.now() };
}

/** Check if a command matches an available tool. */
export function findTool(inventory: McpToolInventory, command: string): McpTool | undefined {
  return inventory.tools.find(t => t.name === command || t.name === command.split(' ')[0]);
}

/** Available slash commands derived from the tool inventory. */
export function getSlashCommands(inventory: McpToolInventory): Array<{ command: string; description: string }> {
  return inventory.tools.map(t => ({ command: `/${t.name}`, description: t.description }));
}
