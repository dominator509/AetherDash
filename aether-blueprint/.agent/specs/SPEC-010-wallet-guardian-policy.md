Layer: 4 - Specification

# SPEC-010: Wallet Guardian Policy

**Status:** accepted | **Owning plans:** EP-306 | **Last updated:** 2026-07-09

## User-visible goal
On-chain actions happen only through a policy machine that simulates first, limits always, and puts a human between agents and anything that moves meaningful value (INV-5).

## Non-goals
Trading logic (router's job); portfolio strategy; non-EVM chains in v1; MPC/enclave custody (documented upgrade path only); key backup ceremonies (operator's password-manager domain, documented in OPERATIONS runbooks).

## Terms
**Guardian** = the isolated signing service (D6: reached only via gRPC, dependency of nothing). **Proposal** = a `ProposeTransaction` lifecycle record. **Hot wallet** = guardian-custody key for small operational funds. **External wallet** = operator's own wallet via WalletConnect v2. **Policy trace** = the ordered rule evaluations attached to every proposal.

## Custody modes (both exist; mode is per-wallet config)
1. **Guardian-custody hot wallet:** key generated inside the Guardian, stored via keystore backend (dev: age-encrypted file at `AETHER_GUARDIAN__KEYSTORE_PATH`, passphrase via systemd credential; prod: same or OS keychain where available). Funding cap: the hot wallet is capped by policy at a small operational balance - topping it up is an external-wallet action.
2. **External via WalletConnect v2:** Guardian is the WC client that assembles and proposes; the operator's wallet app signs. The Guardian's policy STILL evaluates first - WC is a signer, not a bypass.

## Policy model (evaluated in order; first deny wins; all steps recorded in the policy trace)
1. **Chain/asset allowlist:** v1 chains = Ethereum mainnet, Polygon, Arbitrum; assets per-wallet allowlist. Unknown token contract -> deny `asset_not_allowlisted`.
2. **Destination allowlist:** transfers/approvals only to allowlisted addresses. Adding an address = step-up (SPEC-005) + 24 h cooldown before first use (defeats same-session social engineering). Contract interactions allowlist by `(contract, selector)`.
3. **Simulation:** every tx simulates (`eth_call`/`debug_traceCall` per RPC capability) before policy verdict; simulation revert -> deny `simulation_failed` with the revert reason in the trace. Balance-delta from simulation feeds limit checks (what the tx DOES, not what it claims).
4. **Limits:** per-tx max, rolling 24 h aggregate max, per-destination daily max - all in USD terms via the price oracle input (Brain-supplied reference prices with staleness bounds; stale price -> deny `price_stale`).
5. **Approval routing:** below auto-threshold AND destination allowlisted AND not a withdrawal -> `auto_approved` (tier 4/5 actors only). Everything else -> `pending` human approval with step-up TOTP, single-consumption (SPEC-005). **Withdrawals (value leaving the system's wallets to any non-system destination) are ALWAYS human-approved regardless of size** (INV-5, HARD-DENY 6).
6. **Gas policy:** max fee caps per chain; a tx exceeding them -> deny `gas_exceeds_policy` (prevents fee-drain attacks).

## Proposal lifecycle
`pending -> approved (human) | auto_approved | denied | expired` then `broadcast -> confirmed | failed`. Expiry: 10 minutes pending; approved-but-unbroadcast expires in 60 s (approval is for THIS state of the world). Nonce management is Guardian-internal per wallet with a stuck-tx replacement flow (same nonce, bumped fee, policy re-evaluated). Every state change is an audit event; `policy_trace` is stored with the proposal (SPEC-002 gains a `guardian_proposals` table via EP-306's migration - amendment pre-authorized here).

## Required behavior
1. The Guardian MUST expose only the SPEC-003 surface: propose/get/approve. No sign-arbitrary, no key export, no message-signing in v1 (SECURITY.md HARD-DENY 1).
2. Approvals MUST verify: session authenticity, fresh step-up, proposal unexpired, proposal unmodified (approval binds to the proposal hash).
3. A replayed or stale approval MUST fail (`failed_precondition`) - tested per SECURITY.md.
4. The Guardian MUST function with the rest of AETHER down (it only needs its DB rows, RPC endpoints, and keystore) and MUST refuse everything when its keystore is unavailable (fail closed).
5. All RPC endpoints (chain nodes) are operator-configured (ENVIRONMENT.md `AETHER_GUARDIAN__RPC_*` names land with EP-306); public-RPC fallback is allowed for simulation, never for broadcast of guardian-custody txs unless configured so.

## Error states
Closed detail set on denials as listed in the policy model; RPC unreachability -> `unavailable`, no verdict (fail closed); WC pairing loss -> proposals for that wallet `pending` -> `expired` normally.

## Security rules
Key material never leaves the keystore module's memory scope; zeroized after use; no Debug/Display impls on key types; logs and traces carry proposal ids and hashes, never calldata containing sensitive approvals beyond selector + decoded summary. The keystore module is the only code path reading `AETHER_GUARDIAN__KEYSTORE_PATH` (HARD-DENY 1 grep target).

## Required tests
Policy-order table test (each rule denies at its step with correct trace); simulation-revert deny; limit boundary tests (at/over, per-tx/daily/destination); allowlist cooldown test; withdrawal-always-human test at every tier including 5; approval binding test (mutated proposal -> approval fails); stale/replayed approval fails; expiry tests (pending 10 m, approved 60 s); keystore-unavailable refuses-all test; nonce replacement flow test on a local anvil chain fixture.

## Acceptance criteria
EP-306 done = all required tests green (anvil-backed integration), HARD-DENY 1/2/6 verified by failing-by-design tests, the WC pairing flow demonstrated end-to-end on a testnet, and the Guardian systemd unit hardening (DEPLOYMENT.md) applied and smoke-verified.
