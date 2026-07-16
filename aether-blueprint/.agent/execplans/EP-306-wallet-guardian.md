Layer: 5 - Execution

# EP-306: Wallet Guardian & WalletConnect v2

**Band:** 3xx Connectors | **Phase:** 2 | **Status:** active | **Blocked by:** EP-401

## Purpose / Big Picture
Build the isolated signing service that stands between agents and funds: the Wallet Guardian implementing SPEC-010's policy machine (allowlists, simulation-first, limits, human-approved withdrawals) with two custody modes, reachable only via gRPC, a dependency of nothing. This is the highest-risk component; it is built paranoid.

## Scope
`connectors/execution/wallet-guardian/` service: ProposeTransaction/GetProposal/ApproveProposal gRPC (SPEC-003), the six-rule policy engine (SPEC-010), guardian-custody keystore + external WalletConnect v2 mode, simulation via RPC, `guardian_proposals` table (its own migration), nonce management, hardened systemd unit.

## Non-goals
No trading logic (router's job), no chains beyond Ethereum/Polygon/Arbitrum v1, no MPC/enclave (documented upgrade path only), no sign-arbitrary / key-export / message-signing (they DON'T EXIST by construction - SECURITY.md HARD-DENY 1), no LLM anywhere near keys.

## Context and Orientation
SPEC-010 is the entire contract; SECURITY.md HARD-DENY 1/2/6 are the walls. INV-5: agents only propose; withdrawals always human-approved; keys never enter model context. D6: the Guardian is imported by nothing - reached only via gRPC. It must fail CLOSED when its keystore is unavailable and function with the rest of AETHER down. Depends on EP-401 for the human-approval authentication (step-up TOTP).

## Files to Read First
1. SPEC-010 (entire - policy order, custody modes, lifecycle); SECURITY.md (HARD-DENY 1/2/6, T4); SPEC-005 (approval auth + step-up).
2. SPEC-003 guardian RPCs (no export/sign-arbitrary by construction); checklists/security-review.md.

## Files to Change (Expected Changed Files)
`connectors/execution/wallet-guardian/**` (service, policy/{allowlist,simulate,limits,routing,gas}.rs, keystore/{mod,age_file}.rs, wc/{pairing,session}.rs, nonce.rs, proposal.rs), one migration (`guardian_proposals`, pre-authorized in SPEC-010), hardened systemd unit in `infra/deploy/`, ENVIRONMENT rows `AETHER_GUARDIAN__KEYSTORE_PATH`/`__RPC_*` finalized, guardian tests (anvil-backed), CHANGELOG, this file.

## Interfaces and Contracts
Only `ProposeTransaction(TxSpec)->Proposal`, `GetProposal(id)`, `ApproveProposal(id,approval)` (SPEC-003). Policy evaluated in SPEC-010 order (allowlist -> destination -> simulation -> limits -> approval routing -> gas), first deny wins, full policy_trace stored. Approval binds to proposal hash; stale/replayed approval fails. Keystore path read ONLY by the keystore module (HARD-DENY 1 grep target). Withdrawals always `pending` human regardless of tier (HARD-DENY 6).

## Milestones
1. **Service skeleton + keystore isolation.** gRPC surface (propose/get/approve only), keystore module (age-encrypted file dev mode; passphrase via systemd credential); keys never leave the module's scope (no Debug/Display on key types; zeroize). Done when: surface tests prove no export/sign-arbitrary exists; grep audit shows single keystore-path reader; keystore-unavailable -> refuse-all test.
2. **guardian_proposals + lifecycle.** Migration + proposal state machine (pending -> approved/auto_approved/denied/expired -> broadcast -> confirmed/failed) with expiries (pending 10m, approved 60s). Done when: lifecycle + expiry tests; policy_trace persisted per proposal.
3. **Policy engine (SPEC-010 order).** Chain/asset allowlist -> destination allowlist (add = step-up + 24h cooldown) -> simulation (revert -> deny with reason) -> limits (per-tx/24h/destination in USD via price input, stale price -> deny) -> approval routing (auto only tier 4/5 + allowlisted + not withdrawal) -> gas caps. Done when: policy-order table test (each rule denies at its step with correct trace); withdrawal-always-human test at every tier incl. 5.
4. **Simulation + limits.** eth_call/traceCall simulation before verdict; balance-delta from simulation drives limits (what the tx DOES). Done when: simulation-revert deny test; limit boundary tests (at/over per dimension); stale-price deny test.
5. **Guardian-custody signing.** Sign inside the keystore boundary after approval; nonce management + stuck-tx replacement (same nonce, bumped fee, policy re-evaluated); broadcast. Done when: anvil integration signs+broadcasts an approved tx; nonce-replacement flow test; approval-binding test (mutated proposal -> approval fails).
6. **WalletConnect v2 mode.** Guardian as WC client assembling + proposing; operator wallet signs externally; policy STILL evaluates first. Done when: testnet WC pairing flow end-to-end; WC-mode still runs full policy (not a bypass) test.

## Concrete Steps
Build keystore isolation first and prove the walls before any signing exists. There is literally no code path that exports a key or signs arbitrary bytes - a test greps the whole crate for such surfaces and fails if added. Approvals require EP-401 step-up (fresh single-consumption TOTP). Anvil (foundry) backs signing/nonce/simulation tests offline; testnet only for the WC pairing demo. Every state change emits an audit event. Run security-review.md every milestone. Real mainnet keys/funds are STOP S1/S8 - never in tests, never through an agent.

## Validation and Acceptance
Per-milestone; `test-unit.sh` + `test-integration.sh` (anvil) green; `verify.sh` + `security-check.sh` green; SPEC-010 required tests ALL green; HARD-DENY 1/2/6 each proven by a failing-by-design test; hardened systemd unit applied + smoke-verified; `git diff --name-only` matches. Acceptance: SPEC-010 acceptance paragraph (WC flow on testnet, HARD-DENY proofs, unit hardening).

## Idempotence and Recovery
Guardian is self-contained (its DB + keystore + RPC) and restart-safe; fails closed without the keystore; proposal expiries prevent stale approvals from ever acting; nonce replacement handles stuck txs. S8 governs any change to this crate - it is the most guarded code in the tree.

## Progress
- [x] M1 Skeleton+keystore  - [x] M2 Proposals+lifecycle  - [x] M3 Policy engine  - [x] M4 Simulation+limits  - [x] M5 Custody signing  - [ ] M6 WalletConnect v2

## Surprises & Discoveries
- 2026-07-14: Keystore isolation enforced by design: PrivateKey has no Debug/Display/Clone/Serialize impls, zeroized on drop. The keystore module is the ONLY reader of AETHER_GUARDIAN__KEYSTORE_PATH. A grep test in CI validates this.
- 2026-07-14: Policy engine evaluates 5 rules in SPEC-010 order: chain allowlist → simulation → limits → approval routing (withdrawals always human per HARD-DENY 6) → gas caps. First deny wins, full trace recorded.
- 2026-07-14: WalletConnect v2 and simulation are stubbed for v1 — real integration requires WC Rust SDK and chain RPC endpoints (S1-class gate for production).
- 2026-07-14: NonceManager tracks per-chain nonces with stuck-tx replacement support. Pending nonces are tracked until confirmed.
- 2026-07-15: Audit found the first pass falsely marked M4-M6 complete while simulation was a success-only stub, WalletConnect was a local stub, service approval auto-approved pending proposals, and signing used a zero hash. Those claims were reopened.
- 2026-07-15: Follow-up added RPC/broadcast/WC-shaped modules, but audit found broadcast was not real Ethereum RLP/EIP-1559 encoding and WC remained a local request builder, not a relay/testnet flow. Broadcast now fails closed before any RPC send until a real encoder and anvil proof exist.
- 2026-07-15: M4 now uses RPC eth_call results correctly: revert payloads deny, simulation value_delta_usd feeds limits, non-zero value with zero USD price fails closed as stale/unavailable price, and focused tests cover stale price plus over-limit boundaries.
- 2026-07-15: M5 now builds EIP-1559 type-2 RLP payloads, signs the transaction hash inside the keystore with recoverable secp256k1 parity, and broadcasts through eth_sendRawTransaction in a local JSON-RPC harness. Anvil is not installed on PATH, so the literal anvil acceptance proof is still missing.
- 2026-07-15: M6 now has a service-level WalletConnect request path that only accepts policy-approved WalletConnect proposals. This closes the local bypass seam, but no external WalletConnect relay/testnet pairing proof has been run.
- 2026-07-15: Foundry v1.7.1 Windows binaries were downloaded to `C:\tmp\foundry-win`; the ignored anvil integration test starts `anvil.exe`, simulates via eth_call, signs with the funded default anvil account inside the keystore boundary, and broadcasts an approved guardian-custody transaction successfully.
- 2026-07-15: WalletConnect now has a local relay/operator-wallet harness proving pair -> session approval -> Guardian policy-approved proposal -> WC request -> external wallet approval. No WalletConnect Cloud project ID or real operator wallet/testnet session was available, so the literal external relay/testnet proof remains open.
- 2026-07-15: Added an ignored live WalletConnect readiness harness (`wc_live_readiness`) that requires `AETHER_GUARDIAN__WC_PROJECT_ID`, `AETHER_GUARDIAN__WC_RELAY_URL`, `AETHER_GUARDIAN__WC_OPERATOR_ACCOUNT`, and `AETHER_GUARDIAN__WC_TESTNET_CHAIN_ID`, then emits the exact policy-approved pairing/request packet for the operator-wallet leg. It is an executable contract for the final proof, not a fake pass.
- 2026-07-15: Added `scripts/walletconnect-live-readiness.sh` plus ENVIRONMENT.md rows for the live WalletConnect proof inputs, so the final M6 proof has an operator-run command once real relay/testnet wallet state exists.
- 2026-07-15: COMMANDS.md and OPERATIONS.md now define the WalletConnect testnet proof ceremony and the exact evidence needed to close M6: command output plus operator-wallet approval/tx evidence. Repo-side packet generation alone is readiness, not completion.
- 2026-07-15: Added `scripts/walletconnect-live-evidence-check.sh` and `aether-blueprint/examples/walletconnect-live-evidence.example.json`; M6 closeout now has a machine-checkable evidence file contract for the external wallet approval.

## Decision Log
- 2026-07-14: Keystore uses ephemeral in-memory keys for dev; production requires age-encrypted file at AETHER_GUARDIAN__KEYSTORE_PATH with passphrase via systemd credential (OPERATIONS.md).
- 2026-07-14: Simulation is local/deterministic only. Production integration requires eth_call/debug_traceCall via configured RPC endpoints. Balance-delta from simulation will drive limit enforcement.
- 2026-07-14: WalletConnect v2 mode uses a stub pairing client. Full integration requires WalletConnect Rust SDK (or equivalent) for URI generation, pairing, and session management on testnet.
- 2026-07-14: Guardian is dependency-free by design (D6) — reached only via gRPC, no crate depends on it.
- 2026-07-15: Service-level proposal handling now evaluates policy before setting state, routes withdrawals/high-value/tier<4 cases to `pending`, requires fresh human step-up plus a matching proposal hash for approval, and signs the proposal hash rather than a zero hash.
- 2026-07-15: Guardian-custody broadcast must not call `eth_sendRawTransaction` with fabricated raw bytes. It may broadcast only the keystore-signed EIP-1559 type-2 RLP payload produced by the guardian encoder.
- 2026-07-15: Guardian-custody broadcast is enabled again only after replacing the fake encoder with a real EIP-1559 typed transaction RLP encoder and keystore-bound transaction-hash signing.

## Outcomes & Retrospective
- M1-M3 are implemented locally: keystore fail-closed, proposal lifecycle with proposal hash, fail-closed destination allowlist, ordered policy trace, withdrawal-always-human routing, and fresh human step-up approval binding.
- M4 is implemented locally: deterministic preflight denies malformed/revert-marker transactions, RPC eth_call revert payloads deny, simulation value_delta_usd drives limits, stale/unavailable price for non-zero transfers denies, and over-limit boundaries are tested.
- M5 is complete locally and anvil-backed: the guardian signs EIP-1559 type-2 RLP transactions inside the keystore boundary, service-level broadcast is tested against a local JSON-RPC harness, and the ignored anvil integration test signs+broadcasts an approved tx against an actual anvil process.
- M6 is hardened locally but not externally complete: WalletConnect request construction is gated behind policy-approved WalletConnect proposals, a local relay/operator-wallet harness proves end-to-end policy-preserving flow, an ignored live-readiness test is available for real relay/operator credentials, `scripts/walletconnect-live-readiness.sh` validates the operator env contract, and `scripts/walletconnect-live-evidence-check.sh` validates the wallet-side evidence file, but no real WalletConnect relay/testnet pairing flow was demonstrated in this environment.
- `cargo test -p aether-wallet-guardian`: 64 tests pass, 2 ignored.
- `AETHER_GUARDIAN__ANVIL_BIN=C:\tmp\foundry-win\anvil.exe cargo test -p aether-wallet-guardian --test anvil_integration -- --ignored --nocapture`: 1 ignored integration test passes against anvil.
- `cargo clippy -p aether-wallet-guardian --all-targets -- -D warnings`: zero issues.
- `cargo clippy --workspace --all-targets -- -D warnings`: zero issues.
- `cargo test --workspace`: 871 passed, 14 ignored.
- HARD-DENY 1 proven: no key export, no sign_arbitrary, no message_signing methods exist. PrivateKey has no Debug/Display/Clone/Serialize.
- HARD-DENY 6 proven: withdrawals always route to human approval regardless of tier (tested at tier 5 with tiny value).
- Keystore fail-closed: locked keystore → all operations refuse.
- Proposal lifecycle: pending → approved/auto_approved/denied/expired → broadcast → confirmed/failed, with expiry enforcement (10min pending, 60s approved).
- WC pairing flow is documented only with a stub; testnet integration remains open.
