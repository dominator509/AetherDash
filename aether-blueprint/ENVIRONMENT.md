Layer: 3 - Architecture

# ENVIRONMENT.md - Configuration Contract

Names below are the contract: EP-003/EP-004 implement them exactly; new variables are added here in the same plan that introduces them (EXECUTION_RULES R13). Agents never invent variable names (R9).

## Conventions
- Prefix `AETHER_`, sections joined by double underscore: `AETHER_<SECTION>__<KEY>`.
- **Exception:** `DATABASE_URL` keeps its unprefixed name because sqlx's compile-time macros and CLI read it directly (ADR-0004).
- Secrets are values, never defaults: any variable marked (secret) has no default anywhere and its value never appears in logs, examples, or agent output (SECURITY.md).
- Dev values load from `infra/dev/.env.dev` (gitignored); `infra/dev/.env.example` is committed with dummy values. Prod loads via systemd EnvironmentFile/LoadCredential outside the repo (A-16).
- Config precedence per service: process env > env file > built-in dev default. No remote config service in v1.

## Core variables
| Variable | Purpose | Dev default |
|---|---|---|
| DATABASE_URL | Postgres DSN (sqlx) | postgres://aether:aether@localhost:5432/aether |
| AETHER_CLICKHOUSE__URL | ClickHouse HTTP endpoint | http://localhost:8123 |
| AETHER_CLICKHOUSE__DATABASE | ClickHouse database | aether |
| AETHER_CLICKHOUSE__USER | ClickHouse HTTP user | aether (dev only) |
| AETHER_CLICKHOUSE__PASSWORD | ClickHouse HTTP password | aether (dev only; operator supplied outside dev) |
| AETHER_REDIS__URL | Redis/Dragonfly | redis://localhost:6379 |
| AETHER_QDRANT__URL | Qdrant HTTP | http://localhost:6333 |
| AETHER_KAFKA__BROKERS | Redpanda bootstrap | localhost:9092 |
| AETHER_MINIO__ENDPOINT | S3-compatible endpoint | http://localhost:9000 |
| AETHER_MINIO__ACCESS_KEY | (secret) | - |
| AETHER_MINIO__SECRET_KEY | (secret) | - |
| AETHER_KUZU__PATH | Kuzu graph directory (brain host volume) | ./data/kuzu |
| AETHER_BRAIN__RECALL_V2 | Enable recall v2 stages while retaining v1 fallback (`1`/`0`) | 1 |
| AETHER_BRAIN__RECALL_BUDGET_MS | Hard total recall budget, clamped to at most 100 ms | 100 |
| AETHER_BRAIN__RECALL_RERANK | Enable optional local EP-202 cross-encoder rerank (`1`/`0`) | 0 |
| AETHER_BRAIN__RECALL_RERANK_TIMEOUT_MS | Cross-encoder sub-budget, clamped to at most 25 ms | 25 |
| AETHER_ENV | dev / prod | dev |
| AETHER_LOG__LEVEL | trace..error | info |
| AETHER_LOG__FORMAT | json / pretty | pretty (dev), json (prod) |

## Service bind/ports (host-mapped in dev compose; contract for EP-003/EP-004)
| Service | Variable | Dev port |
|---|---|---|
| Postgres | - | 5432 |
| ClickHouse HTTP / native | - | 8123 / 9004 (native remapped; MinIO owns 9000) |
| Redis | - | 6379 |
| Qdrant HTTP / gRPC | - | 6333 / 6334 |
| Redpanda Kafka / admin / proxy | - | 9092 / 9644 / 8082 |
| MinIO API / console | - | 9000 / 9001 |
| Gateway (WS+HTTP) | AETHER_GATEWAY__BIND | 0.0.0.0:8080 |
| Brain API | AETHER_BRAIN__BIND | 127.0.0.1:8000 |
| LLM router (internal) | AETHER_LLM__BIND | 127.0.0.1:8001 |
| MCP server (gateway upstream) | AETHER_MCP__URL | http://127.0.0.1:8000 |
| Alerts service | AETHER_ALERTS__BIND | 127.0.0.1:8002 |
| Inbox webhook receiver | AETHER_INBOX__BIND | 127.0.0.1:8003 |
| Authoritative action effects | AETHER_ACTIONS__BIND | 127.0.0.1:8004 |
| Ingestion fleet | fixed loopback bind | 127.0.0.1:8005 |
| Order router gRPC | AETHER_ROUTER__BIND | 127.0.0.1:50051 |
| Risk engine gRPC | AETHER_RISK__BIND | 127.0.0.1:50052 |
| Wallet Guardian gRPC | AETHER_GUARDIAN__BIND_ADDR | 127.0.0.1:50053 |
| Kalshi adapter gRPC | AETHER_VENUE__KALSHI_GRPC_ADDR | 127.0.0.1:50054 |
| Kalshi health / metrics HTTP | AETHER_VENUE__KALSHI_HEALTH_PORT | 8084 (loopback only) |
| Polymarket adapter gRPC | AETHER_VENUE__POLYMARKET_GRPC_ADDR | 127.0.0.1:50055 |
| Polymarket health / metrics HTTP | AETHER_VENUE__POLYMARKET_HEALTH_PORT | 8085 (loopback only) |
| Hyperliquid adapter gRPC | AETHER_VENUE__HYPERLIQUID_GRPC_ADDR | 127.0.0.1:50056 |
| Hyperliquid health / metrics HTTP | AETHER_VENUE__HYPERLIQUID_HEALTH_PORT | 8086 (loopback only) |
| Alpaca adapter gRPC | AETHER_VENUE__ALPACA_GRPC_ADDR | 127.0.0.1:50057 |
| Alpaca health / metrics HTTP | AETHER_VENUE__ALPACA_HEALTH_PORT | 8087 (loopback only) |
| OpenBB adapter gRPC | AETHER_VENUE__OPENBB_GRPC_ADDR | 127.0.0.1:50058 |
| OpenBB health / metrics HTTP | AETHER_VENUE__OPENBB_HEALTH_PORT | 8088 (loopback only) |
| Prometheus (optional dev) | - | 9090 |

Only the gateway and webhook receivers may ever bind non-loopback in prod; everything else is loopback/WireGuard (SECURITY.md T1).

## Ingestion fleet configuration (EP-206)

| Variable | Purpose | Dev default |
|---|---|---|
| AETHER_INGEST__CONFIG_PATH | Required path to the secret-free JSON source policy file | - |
| AETHER_INGEST__WORKERS | Bounded ingestion worker count | 4 |
| AETHER_INGEST__OCR_ENABLED | Run the parked-screenshot OCR worker (`1`/`0`) | 1 |
| AETHER_INGEST__OCR_ENGINE | OCR backend (`cpu`; optional `gpu` falls back safely) | cpu |
| AETHER_INGEST__OCR_INTERVAL_SECONDS | Pending-screenshot scan interval, minimum 1 second | 5 |

Copy `aether-blueprint/examples/ingest-sources.example.json` to an operator-owned path and replace or disable every placeholder before service start. The JSON stores source policy and maps credential header names to environment-variable names; it must never contain credential values. Startup fails closed when the path is absent, the file is empty, or a source requests bot bypass. The systemd unit binds the audit, health, readiness, and metrics surfaces to loopback port 8005.

## Provider and venue credentials (all secret; STOP S1 when needed and absent)
| Variable | Used by |
|---|---|
| AETHER_LLM__ANTHROPIC_API_KEY / __DEEPSEEK_API_KEY / __XAI_API_KEY / __OPENAI_API_KEY | llm_router |
| AETHER_LLM__LOCAL_ENDPOINT | llm_router (vLLM/Ollama base URL; not secret) |
| AETHER_MCP__URL | gateway; loopback HTTP origin only (no path or credentials) |
| AETHER_SWARM__MAX_COST_PER_TOKEN_USD | swarm; conservative maximum input/output USD per token used for pre-authorization |

Research swarms pre-authorize provider cost using a conservative default ceiling of
`0.0001 USD/token` before dispatch. Provider-reported usage above that reservation
fails closed as `provider_overage`; raise `AETHER_SWARM__MAX_COST_PER_TOKEN_USD`
before enabling a model whose published input or output price exceeds it.
| AETHER_VENUE__KALSHI_KEY_ID | kalshi pack API key identifier |
| AETHER_VENUE__KALSHI_PRIVATE_KEY_PATH | kalshi pack RSA private-key file (readable by that service user only) |
| AETHER_VENUE__KALSHI_BASE_URL | kalshi REST origin (not secret; defaults to `https://external-api.demo.kalshi.co`) |
| AETHER_VENUE__KALSHI_WS_URL | kalshi WebSocket URL (not secret; defaults to `wss://external-api.demo.kalshi.co/trade-api/ws/v2`) |
| AETHER_VENUE__ALPACA_KEY_ID / __ALPACA_SECRET | alpaca pack (paper) |
| AETHER_VENUE__POLYGON_RPC_URL | polymarket pack Polygon RPC endpoint (read-only; not secret; defaults to `https://polygon-rpc.com`) |
| AETHER_VENUE__HYPERLIQUID_INFO_URL | hyperliquid pack Info API endpoint (not secret; defaults to `https://api.hyperliquid.xyz/info`) |
| AETHER_VENUE__ALPACA_BASE_URL | alpaca pack REST origin (not secret; defaults to `https://paper-api.alpaca.markets`) |
| AETHER_VENUE__ALPACA_DATA_URL | alpaca pack Data API origin (not secret; defaults to `https://data.alpaca.markets`) |
| AETHER_VENUE__ALPACA_WS_URL | alpaca pack WebSocket URL (not secret; defaults to `wss://stream.data.alpaca.markets/v2/iex`) |
| AETHER_VENUE__OPENBB_PROVIDER | openbb pack data provider (not secret; defaults to `yfinance`) |
| AETHER_VENUE__OPENBB_* | openbb pack provider-specific API keys (secret; e.g. __POLYGON_API_KEY) |
| AETHER_VENUE__POLYMARKET_GAMMA_URL | polymarket pack Gamma REST origin (not secret; defaults to `https://gamma-api.polymarket.com`) |
| AETHER_VENUE__POLYMARKET_CLOB_URL | polymarket pack CLOB REST origin (not secret; defaults to `https://clob.polymarket.com`) |
| AETHER_VENUE__POLYMARKET_WS_URL | polymarket pack CLOB WebSocket URL (not secret; defaults to `wss://ws-subscriptions-clob.polymarket.com/ws/market`) |
| AETHER_VENUE__HYPERLIQUID_* | hyperliquid pack (read-only in Phase 1; names finalized in EP-303) |
| AETHER_COMMS__TELEGRAM_BOT_TOKEN / __DISCORD_BOT_TOKEN / __SLACK_BOT_TOKEN | alerts |
| AETHER_COMMS__TWILIO_SID / __TWILIO_TOKEN | alerts Twilio account credentials (secret) |
| AETHER_COMMS__TWILIO_FROM / __TWILIO_TO | alerts SMS sender and verified operator destination |
| AETHER_COMMS__TWILIO_WEBHOOK_URL | exact public HTTPS callback URL used for Twilio signature verification |
| AETHER_COMMS__SMTP_HOST / __SMTP_PORT | alerts outbound SMTP endpoint (STARTTLS required; port defaults to 587) |
| AETHER_COMMS__SMTP_USERNAME / __SMTP_PASSWORD | alerts SMTP credentials (secret) |
| AETHER_COMMS__EMAIL_FROM / __EMAIL_TO | alerts sender and verified operator destination; inbound email approvals are disabled |
| AETHER_ALERTS_ACTIONS_URL | authoritative server-plane action service (loopback HTTP or HTTPS only) |
| AETHER_ALERTS_ACTIONS_TOKEN | service credential used only between alerts and the authoritative action service (secret) |
| AETHER_ACTIONS_SERVICE_TOKEN | action-service copy of `AETHER_ALERTS_ACTIONS_TOKEN`; values must match (secret) |
| AETHER_ACTIONS__BIND | authoritative action service bind (loopback only; default `127.0.0.1:8004`) |
| AETHER_ALERTS_INTERNAL_TOKEN | service credential for creating approval prompts through `/internal/approvals` (secret) |
| AETHER_SIMULATOR_BIN | optional path to the canonical `aether-simulator` JSON binary used by MCP `sim.run` |
| AETHER_PAPER_EXECUTOR_BIN | optional path to the canonical `aether-execute-paper` JSON binary used by the action service |
| AETHER_INBOX__GMAIL_AUDIENCE | inbox Gmail Pub/Sub push OIDC audience (not secret) |
| AETHER_INBOX__GMAIL_PUSH_SERVICE_ACCOUNT | inbox expected Pub/Sub push service-account email (not secret) |
| AETHER_INBOX__GMAIL_ACCESS_TOKEN | inbox Gmail API OAuth token (secret) |
| AETHER_INBOX__GMAIL_START_HISTORY_ID | initial Gmail watch cursor; persisted after first successful batch (not secret) |
| AETHER_INBOX__MSGRAPH_CLIENT_STATE | inbox Graph subscription shared verifier (secret) |
| AETHER_INBOX__MSGRAPH_ACCESS_TOKEN | inbox Graph API OAuth token (secret) |
| AETHER_INBOX__DEDUP_DB | inbox durable dedup SQLite path (not secret; default data/inbox-dedup.sqlite3) |
| AETHER_INBOX__QUEUE_DB | inbox durable notification/cursor SQLite path (not secret; default data/inbox-queue.sqlite3) |
| AETHER_GUARDIAN__BIND_ADDR | Guardian gRPC bind; loopback only, default `127.0.0.1:50053` |
| AETHER_GUARDIAN_ENDPOINT | action-service Guardian client endpoint; loopback HTTP only |
| AETHER_GUARDIAN_CLIENT_BIN | path to the no-shell authenticated Guardian gRPC client |
| AETHER_GUARDIAN__SERVICE_TOKEN | internal proposal transport credential; never valid for human approvals (secret) |
| AETHER_GUARDIAN__ALLOWED_DESTINATIONS | operator-defined comma-separated `address@RFC3339-activation` entries; each activation must be at least 24h old and empty denies all |
| AETHER_GUARDIAN__ALLOWED_CONTRACT_CALLS | comma-separated exact `contract:0xselector@RFC3339-activation` entries; the same 24h cooldown applies and plain destination entries never authorize calldata |
| AETHER_GUARDIAN__RPC_1 / __RPC_137 / __RPC_42161 | operator-configured Ethereum, Polygon, and Arbitrum RPC endpoints; missing endpoint fails closed |
| AETHER_GUARDIAN__KEYSTORE_PATH | optional wallet-guardian-only credential-file override; production uses systemd `guardian-keystore` and no env value |
| AETHER_GUARDIAN__WORKER_POLL_MS | durable custody broadcast/reconciliation interval; bounded to 100-60000ms, default 1000ms |
| AETHER_GUARDIAN__WORKER_BATCH_SIZE | maximum due jobs handled per cycle; bounded to 1-100, default 10 |

## Wallet Guardian live WalletConnect proof inputs
These are operator-supplied only for EP-306's live WalletConnect readiness proof. They are not dev defaults and are not required for unit tests. Use `scripts/walletconnect-live-readiness.sh` after the operator has created a WalletConnect project and opened a testnet-capable wallet session.

| Variable | Purpose |
|---|---|
| AETHER_GUARDIAN__WC_PROJECT_ID | WalletConnect project id for the live relay/testnet proof. Treat as operator-controlled config; do not print the value in logs. |
| AETHER_GUARDIAN__WC_RELAY_URL | WalletConnect relay WebSocket URL (`wss://...` or `ws://...`) used by the proof. |
| AETHER_GUARDIAN__WC_OPERATOR_ACCOUNT | Operator wallet address expected to approve the testnet request. |
| AETHER_GUARDIAN__WC_TESTNET_CHAIN_ID | Testnet EVM chain id used by the WalletConnect readiness proof. |

## Execution safety flags
| Variable | Rule |
|---|---|
| AETHER_EXECUTION__LIVE_ENABLED | Default false. Operator-edited out-of-band only; never set by agents, templates, or tests (ADR-0007, HARD-DENY 3). Router refuses live submits when false regardless of caller. |
| AETHER_EXECUTION__PAPER | Default true in dev; paper ledger active. |

## Per-environment differences
- **dev:** compose stack on localhost; pretty logs; paper only; Playwright against `tauri dev`.
- **prod (brain host):** databases via `infra/deploy/compose.prod.yml`; app services as systemd units; JSON logs; WireGuard-only admin; backups per OPERATIONS.md.
- **client (operator desktop):** talks only to gateway URL `AETHER_CLIENT__GATEWAY_URL` (stored in app config, not env); secrets in OS keychain via the Tauri shell (ADR-0008).
