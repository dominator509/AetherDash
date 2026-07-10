# Tech Stack

## Languages & Build
- **Rust** (edition 2024, rustc >= 1.78) — cargo workspace at root, nextest preferred
- **TypeScript** (node >= 20, pnpm >= 9) — pnpm workspace, vitest, tsc --noEmit
- **Python** (>= 3.11, uv) — uv workspace, pytest, mypy, ruff

## Package Managers (lockfiles committed — ADR-0005)
- `Cargo.toml` / `Cargo.lock` (root workspace)
- `pnpm-workspace.yaml` / `pnpm-lock.yaml`
- `pyproject.toml` / `uv.lock`

## Key Frameworks & Libraries
- **Desktop:** Tauri v2 (Rust shell + React/TS frontend)
- **UI:** React, Tailwind CSS, Radix UI
- **Server (Python):** FastAPI, uvicorn, LiteLLM (library mode, not proxy)
- **Server (Rust):** Axum, tonic (gRPC)
- **Databases:** Postgres + pgvector, ClickHouse, Qdrant (vectors), Redis/Dragonfly (cache), Kuzu (embedded graph), MinIO (object lake)
- **Message Bus:** Redpanda (Kafka-compatible)
- **Proto:** buf-managed gRPC contracts in `proto/`
- **Migrations:** sqlx (Rust, paired up/down) + ordered ClickHouse DDL

## Provider & Model Stack
- LLM providers via LiteLLM: Anthropic, DeepSeek, xAI, OpenAI-compatible, local (vLLM/Ollama/llama.cpp)
- Primary coding agent model: DeepSeek (configured in CLAUDE.md)
- Cache-first prompt assembly (INV-3): static blocks first, single cache breakpoint before dynamic data