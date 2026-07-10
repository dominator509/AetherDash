# Environment Configuration

## Naming Convention
`AETHER_<SECTION>__<KEY>` (double underscore between sections)
Exception: `DATABASE_URL` (sqlx convention, unprefixed)

## Secrets
- Marked (secret) in ENVIRONMENT.md — no default anywhere
- Never appear in logs, examples, or agent output
- `AETHER_GUARDIAN__KEYSTORE_PATH` — HARD-DENY on any reader except wallet-guardian

## Config Loading
Precedence: process env > env file > built-in dev default
- Dev: `infra/dev/.env.dev` (gitignored); template: `infra/dev/.env.example`
- Prod: systemd EnvironmentFile/LoadCredential (outside repo)

## Execution Safety
- `AETHER_EXECUTION__LIVE_ENABLED` — default false; operator-edited out-of-band only; never set by agents/templates/tests (ADR-0007, HARD-DENY 3)
- `AETHER_EXECUTION__PAPER` — default true in dev

## Binding Rules
- Only gateway and webhook receivers bind non-loopback in prod
- Everything else: loopback/WireGuard (SECURITY.md T1)

## Key Variables (dev defaults)
- DATABASE_URL: postgres://aether:aether@localhost:5432/aether
- AETHER_CLICKHOUSE__URL: http://localhost:8123
- AETHER_REDIS__URL: redis://localhost:6379
- AETHER_QDRANT__URL: http://localhost:6333
- AETHER_KAFKA__BROKERS: localhost:9092
- AETHER_MINIO__ENDPOINT: http://localhost:9000
- AETHER_ENV: dev
See ENVIRONMENT.md for full table.