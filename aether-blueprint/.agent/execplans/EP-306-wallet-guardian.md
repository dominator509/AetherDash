Layer: 5 - Execution

# EP-306: Wallet Guardian & WalletConnect v2

**Band:** 3xx Connectors | **Phase:** 2 | **Status:** draft | **Blocked by:** EP-401

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
- [ ] M1 Skeleton+keystore  - [ ] M2 Proposals+lifecycle  - [ ] M3 Policy engine  - [ ] M4 Simulation+limits  - [ ] M5 Custody signing  - [ ] M6 WalletConnect v2

## Surprises & Discoveries
(anvil simulation fidelity; WC v2 pairing realities; nonce edge cases)

## Decision Log
(keystore backend for prod; price-oracle input source; WC library choice)

## Outcomes & Retrospective
(HARD-DENY proof bundle; testnet WC evidence; hardening applied)
