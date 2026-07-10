# Suggested Commands

## Verification (run via Git Bash or WSL)
```bash
scripts/preflight.sh          # toolchain check
scripts/install.sh            # deps install
scripts/lint.sh               # clippy + eslint + ruff
scripts/format-check.sh       # cargo fmt --check + prettier --check + ruff format --check
scripts/typecheck.sh          # tsc --noEmit + mypy
scripts/test-unit.sh          # nextest + vitest + pytest (not integration/e2e)
scripts/test-integration.sh   # requires Docker dev stack
scripts/test-e2e.sh           # Playwright (active after EP-101)
scripts/build.sh              # cargo build + pnpm build + python compileall
scripts/security-check.sh     # gitleaks + forbidden-path + import-boundary grep
scripts/dependency-audit.sh   # cargo audit + pnpm audit + pip-audit
scripts/smoke-test.sh         # dev stack health checks
scripts/verify.sh             # preflight -> format-check -> lint -> typecheck -> unit -> build
scripts/production-readiness-check.sh  # full gate
```

## Greenfield Gating
Each stack's commands become ACTIVE only once its marker file exists: `Cargo.toml`, `pnpm-workspace.yaml`, `pyproject.toml`, `infra/dev/docker-compose.yml`. Missing marker → SKIP notice (not failure).

## Windows-Specific
- All `scripts/*.sh` require Git Bash (`C:\Program Files\Git\bin\bash.exe`) or WSL
- RTK wraps all Bash commands automatically via hook (no manual `rtk` prefix needed)
- Path separators: use `/` in Git Bash, `\` in PowerShell
- `uv` and `pnpm` available in both PowerShell and Git Bash
- Docker Desktop required for dev compose stack

## RTK Usage
- `rtk gain` — token savings analytics
- `rtk gain --history` — command usage history with savings
- `rtk discover` — analyze Claude Code history for missed opportunities
- `rtk proxy <cmd>` — execute raw command without filtering
- All other commands auto-wrapped via hook; no manual prefix needed