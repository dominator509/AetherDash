Layer: 6 - Verification & Operations

# Checklist: Changing the Execution Path (router / risk / simulator / guardian / venue order code)

- [ ] Change is scoped by the active ExecPlan and stays in the connectors/execution (or named) area.
- [ ] Unit tests for the changed logic.
- [ ] Replay test exercising the change against recorded fixtures (deterministic: same recording -> same decisions).
- [ ] Lifecycle assertion updated if the opportunity flow changed (chains still close, TESTING.md).
- [ ] Risk engine: every affected rejection reason has firing + non-misfiring tests.
- [ ] Net-edge math changes: golden vectors updated with hand-computed values; sum law + explicit-zero law hold.
- [ ] Fail-closed posture preserved: any doubt -> no order (SPEC-006); no auto-retry on submit; idempotency key honored.
- [ ] No LLM/MCP call introduced on the path (INV-1/INV-2); no venue-specific branch in core (INV-7).
- [ ] `live_enabled` remains unreachable to agents; caps unchanged (S7).
- [ ] Simulator/paper-ledger fill-model parity preserved (shared implementation).
- [ ] `scripts/security-check.sh` D3 boundary still clean.
