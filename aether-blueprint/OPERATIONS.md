Layer: 6 - Verification & Operations

# OPERATIONS.md - Running AETHER

Operator-facing procedures. Agents may reference these but never execute the sections marked HUMAN.

## Start / stop / status (brain host)
```
docker compose -f /opt/aether/compose/compose.prod.yml up -d --wait   # databases
systemctl start 'aether-*'        # app services (dependency order handled by unit Wants/After)
systemctl status 'aether-*'
systemctl stop 'aether-*'         # databases stay up unless maintenance requires otherwise
journalctl -u aether-<service> -f # logs (JSON in prod; jq-friendly)
```
Dev equivalents live in COMMANDS.md "Local start".

The alerts action-effect path is loopback-only in the standard deployment: alerts calls `http://127.0.0.1:8004`, both units receive matching `AETHER_ALERTS_ACTIONS_TOKEN`/`AETHER_ACTIONS_SERVICE_TOKEN` values from the root-owned environment file, and the action service invokes `/usr/local/bin/aether-execute-paper`. Apply migration 0034 and provision explicit `paper_balances` rows before paper Execute can succeed; missing balances, caps, grants, approvals, Postgres, Redpanda, or executor binaries fail closed.

The Guardian approval path is also loopback-only: the authenticated client sends its session bearer, single-use comms reference, and current TOTP to `/v1/guardian/approve`; the action service passes them on stdin to `/usr/local/bin/guardian-client`; that client calls only the Guardian's gRPC `GetProposal` and `ApproveProposal` methods. The Guardian independently rechecks the human session, current tier-4+ `guardian.approve` grant, proposal hash, unconsumed approval reference, bound step-up challenge, and TOTP before atomically consuming both references and approving the proposal. A bare SMS/email response and a caller-supplied `step_up_satisfied` boolean never authorize this path.

## Guardian credential provisioning (HUMAN; S8)
Provision these on the brain host without sending their contents through an agent or placing them in an environment file:
1. Generate the hot-wallet secp256k1 key outside AETHER and store its 64 hexadecimal characters as a systemd encrypted credential at `/opt/aether/shared/credentials/guardian-keystore.cred` using `systemd-creds encrypt`. Never use a funded production key for development.
2. Store the authenticator's base32 TOTP secret as `/opt/aether/shared/credentials/operator-totp.cred` through the same interactive `systemd-creds` flow. Set the operator's `users.totp_secret_ref` to the credential name `operator-totp`; never store the secret itself in Postgres.
3. Keep the credential directory root-owned mode `0700`; the unit's `LoadCredentialEncrypted=` directives expose decrypted bytes only in its private runtime credential directory.
4. Apply migrations 0035, 0036, and 0039; configure the loopback bind, service token, and per-chain RPC endpoints. Adding a destination or `(contract, selector)` requires a human step-up recorded in the ops log, followed by a full 24-hour cooldown. Only after that cooldown may the root-owned environment receive the entry as `address@RFC3339-activation` or `contract:selector@RFC3339-activation`; the Guardian rejects missing or younger activation timestamps at startup. Then start `aether-wallet-guardian`. The trusted price-ingestion path must append fresh rows to `guardian_reference_prices` using chain-bound IDs (`eip155:<chain>/native` or `eip155:<chain>/erc20:<address>`); proposal callers cannot supply their own price or precision.
5. Confirm gRPC health on port 50053. The in-process custody worker claims approved rows into immutable `guardian_broadcast_jobs`, allocates chain nonces under a Postgres advisory lock, persists the exact signed raw transaction and its locally derived hash before sending, and reconciles `eth_getTransactionByHash`/receipts after restarts. Never delete or edit job rows to force a retry; restoring the RPC lets the worker safely resend the same bytes. A prepared job that was never observed on-chain may expire and releases only its unused tail nonce.
6. Missing Postgres, RPC, encrypted credentials, allowlist entries, fresh reference prices, current grants, references, or TOTP enrollment must leave the Guardian unavailable, defer the durable job, or deny the action. Inspect only non-secret state with `SELECT proposal_id,chain_id,nonce,tx_hash,state,attempts,last_error_code FROM guardian_broadcast_jobs ORDER BY created_ts DESC;`; do not log `signed_raw`.

## EP-307 paper-run closure evidence (HUMAN)
After a continuous paper run of at least 24 hours, run `cargo run -p aether-scanner --bin ep307-evidence` with the production-read replica `DATABASE_URL`. Exit 0 requires opportunity activity in every hourly bucket, zero open chains, zero missing attribution rows, and at least one executed paper chain. Save its JSON output with the ops log. The deterministic three-venue accelerated replay proves restart, dedupe, expiry, and attribution behavior, but it does not replace this wall-clock acceptance run.

The scanner requires explicit tick/book topic allowlists and publishes only after its scored row and outbox are durable. Keep `AETHER_GATEWAY_BUS_ENABLED=1` so the gateway consumes detections, appends the guarded `scored -> surfaced` event, and emits a stable `feed_item` frame. OpenBB is a read-only evidence provider and must never be listed as an executable book venue. Venue fee TOMLs are conservative paper fallbacks; execution adapters must replace them with current market/account rates before live routing.

## Ops log
Append-only Markdown at `/opt/aether/shared/ops-log.md`: timestamp, actor (human name or agent+plan), action, result. Deploys, config changes, backups/restores, incidents, and ceremonies all get a line. The audit chain covers in-system events; the ops log covers operator actions around the system.

## Backups (nightly, systemd timers; built in EP-407, contract fixed here)
| Store | Method | Target | Retention |
|---|---|---|---|
| Postgres | `pg_dump -Fc aether` | MinIO `aether-backups/pg/<date>.dump` | 30 daily + 12 monthly |
| ClickHouse | `BACKUP DATABASE aether TO Disk('backups', '<date>')` then sync to MinIO `aether-backups/ch/` | same | 14 daily |
| Kuzu | flush via brain admin endpoint, then tar of `shared/data/kuzu` | MinIO `aether-backups/kuzu/` | 14 daily |
| MinIO (raw lake + backups bucket) | mirror to second disk or offsite via `mc mirror` | operator-chosen | continuous |
| Qdrant | snapshot API per collection | MinIO `aether-backups/qdrant/` | 7 daily |
| Redis / Redpanda | not backed up: cache and replayable streams respectively (Decision: acceptable v1 loss; revisit if bus becomes system-of-record for anything) | - | - |

**Restore drill (HUMAN, quarterly):** restore latest Postgres dump into a scratch database, run row-count sanity queries, record in ops log. A backup that has never been restored is not a backup.

## Scheduled jobs (systemd timers; definitions land with their owning plans)
- Nightly: backups (above); `dependency-audit.sh` report; Brain tiering/decay/archival pass (EP-201/207); vault view regeneration (INV-9).
- Hourly: audit-chain incremental verify (EP-402); staleness sweep marking expired Brain objects.
- Every 5 min: source-health checks for venue feeds (adapters export `feed_lag`; see OBSERVABILITY.md).

## Disk and data hygiene
ClickHouse ticks: 90 days full resolution, then downsampled (SPEC-002 retention). MinIO raw lake grows unbounded by design; monitor `minio_bucket_usage` and expand storage rather than delete raw provenance. Alert at 80% disk on any volume.

## First live trade ceremony (HUMAN; ADR-0007, HARD-DENY 3)
Preconditions: Phase 2 exit criteria met on paper; caps configured and verified in DB; audit chain verifying clean.
1. Announce in ops log: venue, market, max size (small), rationale.
2. Edit `shared/env/order-router.env`: `AETHER_EXECUTION__LIVE_ENABLED=true`; restart `aether-order-router`; confirm router logs the flag flip with an audit entry.
3. Step-up 2FA in the client; place ONE order at minimum viable size via the normal flow (not a bypass).
4. Verify: fill recorded, audit entry chained, P&L attribution row exists, alert confirmation received.
5. Decide: keep enabled (log it) or revert the flag (log it). Either way the ceremony record links the audit entry IDs.

## WalletConnect testnet proof ceremony (HUMAN; EP-306 M6)
Preconditions: WalletConnect project created by the operator, testnet-capable operator wallet ready, no mainnet funds involved, and the four proof variables in ENVIRONMENT.md supplied from an operator-controlled env file or shell. If using MetaMask Mobile, enable **Show test networks**, select the configured testnet (Sepolia for chain 11155111), and select the configured account before scanning. Agents may prepare the command and inspect non-secret output; the operator controls the wallet approval.
1. Set `AETHER_GUARDIAN__WC_PROJECT_ID`, `AETHER_GUARDIAN__WC_RELAY_URL`, `AETHER_GUARDIAN__WC_OPERATOR_ACCOUNT`, and `AETHER_GUARDIAN__WC_TESTNET_CHAIN_ID`.
2. Run `scripts/walletconnect-live-readiness.sh` and scan the topic-stamped standalone image path it prints (`data/walletconnect-pairing-<topic-prefix>.png`). The terminal QR is a fallback; every run creates a uniquely named image and previous pairings expire. The raw pairing URI is intentionally not printed because it contains the pairing symmetric key.
3. Approve the session only if it requests the configured testnet, `eth_sendTransaction`, and the configured operator account.
4. Verify the wallet shows the expected `eth_sendTransaction` request, destination, value, and chain id; reject if anything differs.
5. Approve externally in the wallet. The client accepts only a 32-byte transaction hash, writes `data/walletconnect-live-evidence.json` with a hash commitment instead of the pairing URI/symmetric key, validates it, and disconnects the proof session.
6. Record the generated non-secret evidence path and transaction hash in the ops log. Never record a project secret, seed phrase, private key, or wallet recovery material.
7. EP-306 M6 may be marked complete only when the generated evidence file passes `walletconnect-live-evidence-check.sh`; local packet generation is not completion.

## Incident basics
Severity: SEV1 = money or keys at risk (freeze first: stop `aether-order-router` and `aether-wallet-guardian`, then diagnose); SEV2 = trading path degraded (stale feeds, router rejects-all); SEV3 = everything else. Runbooks per `runbook-template.md` are required for: key exposure, venue credential leak, poisoned ingest (SECURITY.md), feed outage, database disk-full, audit-chain verify failure. Runbook set must exist before Phase 2 exit.

## Routine health review (HUMAN, weekly, ~10 min)
Golden-signal dashboard (OBSERVABILITY.md), alert precision trend, LLM cache-hit and cost per opportunity, backup timer status, disk headroom, dependency-audit nightly report, open `revise` entries in GENERATION-STATE/PLANS.
