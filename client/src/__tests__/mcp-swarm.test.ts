import { afterEach, describe, expect, it, vi } from "vitest";

import { formatDecisionPacket, launchSwarm, type DecisionPacket } from "../lib/mcp";

const request = {
  question: "Should we enter?",
  budget: { max_calls: 1, max_tokens: 1000, max_cost_usd: "1", max_seconds: 5 },
};

afterEach(() => vi.unstubAllGlobals());

describe("swarm command-room transport", () => {
  it("round-trips confirmation and renders one cited proposal packet", async () => {
    const packet: DecisionPacket = {
      recommendation: {
        text: "Proceed cautiously.",
        citations: [{ object_id: "brain-1", provenance_hash: "hash-1" }],
      },
      confidence: 0.75,
      rationale: [
        {
          text: "Evidence supports entry.",
          citations: [{ object_id: "brain-1", provenance_hash: "hash-1" }],
        },
      ],
      citations: [{ object_id: "brain-1", provenance_hash: "hash-1" }],
      budget_used: { calls: 1, tokens: 20, cost_usd: "0.01", elapsed_seconds: 0.1 },
      budget_truncated: false,
      proposal_only: true,
    };
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({ details: "confirmation_ref=opaque-ref", code: "failed_precondition" }),
          { status: 412, headers: { "Content-Type": "application/json" } },
        ),
      )
      .mockResolvedValueOnce(
        new Response(
          [
            JSON.stringify({ type: "progress", event: { kind: "started" } }),
            JSON.stringify({ type: "progress", event: { kind: "packet_ready" } }),
            JSON.stringify({ type: "packet", result: packet }),
            "",
          ].join("\n"),
          { status: 200, headers: { "Content-Type": "application/x-ndjson" } },
        ),
      );
    vi.stubGlobal("fetch", fetchMock);

    const challenge = await launchSwarm("http://gateway", "token", request);
    expect(challenge.status).toBe("confirmation_required");
    if (challenge.status !== "confirmation_required") throw new Error("expected challenge");
    const streamed: string[] = [];
    const completed = await launchSwarm(
      "http://gateway",
      "token",
      request,
      challenge.confirmationRef,
      (event) => streamed.push(event.kind),
    );
    expect(completed.status).toBe("completed");
    if (completed.status !== "completed") throw new Error("expected packet");
    const rendered = formatDecisionPacket(completed.packet);
    expect(rendered).toContain("brain-1");
    expect(rendered).toContain("Proposal only — no action was executed.");
    expect(streamed).toEqual(["started", "packet_ready"]);
    expect(completed.progress).toHaveLength(2);
    expect(fetchMock.mock.calls[0]?.[0]).toBe("http://gateway/mcp/tools/swarm.launch/stream");
    expect(fetchMock.mock.calls[1]?.[1]?.body).toContain('"confirmation_ref":"opaque-ref"');
  });

  it("rejects a stream that emits a second decision packet", async () => {
    const packet = {
      recommendation: { text: "x", citations: [{ object_id: "b", provenance_hash: "h" }] },
      confidence: 0.5,
      rationale: [{ text: "x", citations: [{ object_id: "b", provenance_hash: "h" }] }],
      citations: [{ object_id: "b", provenance_hash: "h" }],
      budget_used: { calls: 1, tokens: 1, cost_usd: "0", elapsed_seconds: 0 },
      budget_truncated: false,
      proposal_only: true,
    };
    vi.stubGlobal(
      "fetch",
      vi
        .fn()
        .mockResolvedValue(
          new Response(
            `${JSON.stringify({ type: "packet", result: packet })}\n${JSON.stringify({ type: "packet", result: packet })}\n`,
            { status: 200 },
          ),
        ),
    );
    await expect(launchSwarm("http://gateway", "token", request)).rejects.toThrow("more than one");
  });
});
