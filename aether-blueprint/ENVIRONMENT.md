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
| Alerts service | AETHER_ALERTS__BIND | 127.0.0.1:8002 |
| Inbox webhook receiver | AETHER_INBOX__BIND | 127.0.0.1:8003 |
| Order router gRPC | AETHER_ROUTER__BIND | 127.0.0.1:50051 |
| Risk engine gRPC | AETHER_RISK__BIND | 127.0.0.1:50052 |
| Wallet Guardian gRPC | AETHER_GUARDIAN__BIND | 127.0.0.1:50053 |
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

## Provider and venue credentials (all secret; STOP S1 when needed and absent)
| Variable | Used by |
|---|---|
| AETHER_LLM__ANTHROPIC_API_KEY / __DEEPSEEK_API_KEY / __XAI_API_KEY / __OPENAI_API_KEY | llm_router |
| AETHER_LLM__LOCAL_ENDPOINT | llm_router (vLLM/Ollama base URL; not secret) |
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
| AETHER_COMMS__TWILIO_SID / __TWILIO_TOKEN / __TWILIO_FROM | alerts (Phase 2, EP-308) |
| AETHER_INBOX__GMAIL_AUDIENCE | inbox Gmail Pub/Sub push OIDC audience (not secret) |
| AETHER_INBOX__GMAIL_PUSH_SERVICE_ACCOUNT | inbox expected Pub/Sub push service-account email (not secret) |
| AETHER_INBOX__GMAIL_ACCESS_TOKEN | inbox Gmail API OAuth token (secret) |
| AETHER_INBOX__GMAIL_START_HISTORY_ID | initial Gmail watch cursor; persisted after first successful batch (not secret) |
| AETHER_INBOX__MSGRAPH_CLIENT_STATE | inbox Graph subscription shared verifier (secret) |
| AETHER_INBOX__MSGRAPH_ACCESS_TOKEN | inbox Graph API OAuth token (secret) |
| AETHER_INBOX__DEDUP_DB | inbox durable dedup SQLite path (not secret; default data/inbox-dedup.sqlite3) |
| AETHER_INBOX__QUEUE_DB | inbox durable notification/cursor SQLite path (not secret; default data/inbox-queue.sqlite3) |
| AETHER_GUARDIAN__KEYSTORE_PATH | wallet-guardian only; HARD-DENY on any other reader |

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
