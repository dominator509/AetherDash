Layer: 5 - Execution

# EP-103: Command Room Harness

**Band:** 1xx Client | **Phase:** 1 | **Status:** draft | **Blocked by:** EP-101, EP-202

## Purpose / Big Picture
Turn the palette into a control surface: an MCP client in the shell, a chat-style command room (Advanced) plus palette slash-commands (Simple), the always-visible tier badge, and the rule that makes INV-1 true at the UI - model output NEVER auto-executes; every proposed action renders as a card that goes through the same confirm flow as a manual ticket.

## Scope
MCP client over the gateway/MCP transport, command room surface (NL + slash), tier badge, action-card renderer with confirm flow, tool-inventory awareness (render only tools the session's tier exposes), swarm launch entry point (budget field; swarm itself is EP-205), streaming assistant responses.

## Non-goals
No swarm orchestration (EP-205), no plugin UI (EP-403), no live execution semantics beyond the confirm-card mechanism (router is EP-305), no real LLM routing (that's server-side EP-202, which this depends on for the command backend).

## Context and Orientation
SPEC-004 surface 5 is the contract; SPEC-003 MCP inventory is tier-filtered SERVER-side (the client renders what it's given and must not assume unlisted tools). The load-bearing rule: assistant text is never an action; only confirm-cards are, and they reuse EP-101's confirm flow. Tier badge reflects server-provided session tier, never client config (SPEC-005).

## Files to Read First
1. SPEC-004 surface 5; SPEC-003 MCP tool inventory + `command`/`command_result`/`confirm_required` frames.
2. SPEC-005 tiers + step-up (what the badge means, when TOTP appears).
3. EP-101 confirm-flow shell + WS dispatch.

## Files to Change (Expected Changed Files)
`client/src/surfaces/command-room/**`, `client/src/lib/mcp.ts`, `client/src/components/{action-card,tier-badge,slash-menu}/**`, palette slash-command registration, WS/command wiring, `client/e2e/command-room.spec.ts`, vitest suites, CHANGELOG, this file.

## Interfaces and Contracts
`command {text, room_context}` -> streamed `command_result` (assistant text) + zero-or-more `confirm_required` (action cards). Slash commands map to MCP tools; the client requests the tier-filtered inventory and renders only what it receives. Action card -> same `confirm {ref_id, totp?}` path as the ticket; step-up TOTP field appears when the card demands it (server signals via `confirm_required.tier_reason`).

## Milestones
1. **MCP client + inventory.** `mcp.ts` fetches the tier-filtered tool list; renders available slash-commands from it (unknown/unlisted tools simply don't appear). Done when: e2e with a stubbed tier-1 vs tier-3 grant shows different command sets (client reflects server, no client-side gating logic).
2. **Command room surface.** Chat transcript, input, streaming assistant text from `command_result`, room context (current surface/selection) attached to `command`. Done when: e2e sends a command, asserts streamed render + context inclusion.
3. **Action cards + confirm.** Assistant-proposed actions arrive as `confirm_required` and render as cards (action summary, tier reason, paper/live badge); confirming reuses EP-101 confirm flow incl. TOTP when demanded; assistant text alone NEVER triggers an action. Done when: e2e proves (a) a card requires explicit confirm, (b) text mentioning an action does not execute anything, (c) TOTP field appears on a step-up card.
4. **Tier badge + Simple parity.** Always-visible badge from server session state; Simple mode exposes the same commands via palette slash-menu (no chat transcript). Done when: e2e asserts badge reflects a server-changed tier and Simple slash-menu invokes the same tool with the same confirm.
5. **Swarm entry.** `swarm.launch {budget}` command with a budget field; renders a pending swarm placeholder (orchestration is EP-205 - contract-test the launch frame + budget, stub the result). Done when: launch frame shape tested; stub documented.

## Concrete Steps
Reuse confirm flow and WS dispatch from EP-101. Keep the "text is not action" rule enforced structurally: the transcript renderer has no execution capability at all; only the card component can emit a `confirm`. Add a test that feeds an assistant message containing an imperative ("I'll submit the order") and asserts zero intents emitted. Where EP-202's command backend is thin, drive via a gateway command stub echoing SPEC-003 frames + Decision Log. Commit per milestone.

## Validation and Acceptance
Per-milestone; `test-unit.sh` + `test-e2e.sh` green; `verify.sh` -> `verify: ok`; the INV-1-at-UI test (text never executes) is REQUIRED and named; tier-reflection test proves no client-side authorization logic; `git diff --name-only` matches. Acceptance: SPEC-004 surface 5 behaviors demonstrated.

## Idempotence and Recovery
Command room transcript is view state (server holds no client transcript in v1 - Decision Log if any persistence is added); reconnect clears in-flight streams cleanly. Stub seams for EP-202/205 are replaced when those land.

## Progress
- [ ] M1 MCP client+inventory  - [ ] M2 Command room  - [ ] M3 Action cards+confirm  - [ ] M4 Tier badge+Simple  - [ ] M5 Swarm entry

## Surprises & Discoveries
(MCP transport realities through the gateway; streaming frame handling)

## Decision Log
(command backend stub contract; transcript persistence stance)

## Outcomes & Retrospective
(command surface demonstrated; INV-1-at-UI evidence; stubs for EP-202/205)
