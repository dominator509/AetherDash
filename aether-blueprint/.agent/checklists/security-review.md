Layer: 6 - Verification & Operations

# Checklist: Security Review (any security-relevant change)

- [ ] `scripts/security-check.sh` passes (forbidden paths, secret scan, D3 boundary).
- [ ] No secret value in repo, logs, test output, recordings, prompts/model context, or agent response.
- [ ] Trust-boundary input validated (venue payload / inbox attachment / plugin manifest / WS frame) per SPEC-006.
- [ ] HARD-DENY inventory (SECURITY.md) untouched; if the change is near one, a failing-by-design test proves the line holds.
- [ ] Permission tier enforced server-side at the right point(s) (gateway/router/MCP/Guardian), not just UI (SPEC-005).
- [ ] New logging routes through the redaction layer; no request bodies/headers of authenticated calls logged.
- [ ] Wallet/key material: no new reader of keystore paths; nothing places key bytes into logs or prompts (INV-5).
- [ ] Execution safety: no path can submit live orders, flip `live_enabled`, or alter caps/geofencing (S7).
- [ ] New dependencies audited (`scripts/dependency-audit.sh`); execution/wallet deps have ADR-style justification.
