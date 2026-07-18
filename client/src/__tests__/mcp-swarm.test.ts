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
        new Response(JSON.stringify({ result: packet, progress: [{ kind: "packet_ready" }] }), {
          status: 200,
          headers: { "Content-Type": "application/json" },
        }),
      );
    vi.stubGlobal("fetch", fetchMock);

    const challenge = await launchSwarm("http://gateway", "token", request);
    expect(challenge.status).toBe("confirmation_required");
    if (challenge.status !== "confirmation_required") throw new Error("expected challenge");
    const completed = await launchSwarm(
      "http://gateway",
      "token",
      request,
      challenge.confirmationRef,
    );
    expect(completed.status).toBe("completed");
    if (completed.status !== "completed") throw new Error("expected packet");
    const rendered = formatDecisionPacket(completed.packet);
    expect(rendered).toContain("brain-1");
    expect(rendered).toContain("Proposal only — no action was executed.");
    expect(fetchMock.mock.calls[1]?.[1]?.body).toContain('"confirmation_ref":"opaque-ref"');
  });
});
