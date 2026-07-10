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
| Prometheus (optional dev) | - | 9090 |

Only the gateway and webhook receivers may ever bind non-loopback in prod; everything else is loopback/WireGuard (SECURITY.md T1).

## Provider and venue credentials (all secret; STOP S1 when needed and absent)
| Variable | Used by |
|---|---|
| AETHER_LLM__ANTHROPIC_API_KEY / __DEEPSEEK_API_KEY / __XAI_API_KEY / __OPENAI_API_KEY | llm_router |
| AETHER_LLM__LOCAL_ENDPOINT | llm_router (vLLM/Ollama base URL; not secret) |
| AETHER_VENUE__KALSHI_API_KEY_ID / __KALSHI_PRIVATE_KEY_PATH | kalshi pack (key file readable by that service user only) |
| AETHER_VENUE__ALPACA_KEY_ID / __ALPACA_SECRET | alpaca pack (paper) |
| AETHER_VENUE__POLYGON_RPC_URL | polymarket pack (read-only) |
| AETHER_VENUE__HYPERLIQUID_* | hyperliquid pack (read-only in Phase 1; names finalized in EP-303) |
| AETHER_COMMS__TELEGRAM_BOT_TOKEN / __DISCORD_BOT_TOKEN / __SLACK_BOT_TOKEN | alerts |
| AETHER_COMMS__TWILIO_SID / __TWILIO_TOKEN / __TWILIO_FROM | alerts (Phase 2, EP-308) |
| AETHER_INBOX__GMAIL_* / __MSGRAPH_* | inbox (names finalized in EP-204) |
| AETHER_GUARDIAN__KEYSTORE_PATH | wallet-guardian only; HARD-DENY on any other reader |

## Execution safety flags
| Variable | Rule |
|---|---|
| AETHER_EXECUTION__LIVE_ENABLED | Default false. Operator-edited out-of-band only; never set by agents, templates, or tests (ADR-0007, HARD-DENY 3). Router refuses live submits when false regardless of caller. |
| AETHER_EXECUTION__PAPER | Default true in dev; paper ledger active. |

## Per-environment differences
- **dev:** compose stack on localhost; pretty logs; paper only; Playwright against `tauri dev`.
- **prod (brain host):** databases via `infra/deploy/compose.prod.yml`; app services as systemd units; JSON logs; WireGuard-only admin; backups per OPERATIONS.md.
- **client (operator desktop):** talks only to gateway URL `AETHER_CLIENT__GATEWAY_URL` (stored in app config, not env); secrets in OS keychain via the Tauri shell (ADR-0008).
