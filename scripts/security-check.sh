#!/usr/bin/env bash
# Layer: 6 - Verification & Operations
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"
FAIL=0

# --- 1. Forbidden tracked paths (AGENTS.md section 12; INV-5) ---
if git rev-parse --is-inside-work-tree >/dev/null 2>&1; then FILES="$(git ls-files)"; else FILES="$(find . -type f -not -path './.git/*' | sed 's|^\./||')"; fi
BAD_PATHS="$(printf '%s\n' "$FILES" | grep -E '(^|/)\.env$|\.pem$|\.key$|(^|/)id_(rsa|ed25519|ecdsa)' || true)"
if [ -n "$BAD_PATHS" ]; then echo "FAIL: forbidden files tracked:"; printf '%s\n' "$BAD_PATHS"; FAIL=1; fi

# --- 2. Secret pattern scan ---
if command -v gitleaks >/dev/null 2>&1 && git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  gitleaks detect --no-banner --source . || FAIL=1
else
  PAT='AKIA[0-9A-Z]{16}|-----BEGIN [A-Z ]*PRIVATE KEY|sk-[A-Za-z0-9]{20,}|xox[baprs]-[A-Za-z0-9-]{10,}|ghp_[A-Za-z0-9]{36}'
  if git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    HITS="$(git grep -nE "$PAT" -- ':!*.lock' ':!scripts/security-check.sh' || true)"
  else
    HITS="$(grep -RInE "$PAT" . \
        --exclude-dir=.git --exclude-dir=node_modules --exclude-dir=target \
        --exclude-dir=vault --exclude-dir=.venv --exclude='*.lock' \
        --exclude='security-check.sh' || true)"
  fi
  if [ -n "$HITS" ]; then echo "FAIL: secret-like content:"; printf '%s\n' "$HITS"; FAIL=1; fi
fi

# --- 3. Boundary grep D3 (ARCHITECTURE.md 11): execution plane must not touch LLM/MCP code ---
if [ -d connectors/execution ]; then
  BND="$(grep -RInE '^(use|pub use) .*(mcp|anthropic|openai|litellm|deepseek)|^[[:space:]]*(mcp|anthropic|openai|litellm|deepseek)[-_a-zA-Z0-9]*[[:space:]]*=' \
      connectors/execution --include='*.rs' --include='Cargo.toml' || true)"
  if [ -n "$BND" ]; then echo "FAIL: D3 violation (LLM/MCP in execution plane):"; printf '%s\n' "$BND"; FAIL=1; fi
fi

[ "$FAIL" -eq 0 ] || exit 1
echo "security: ok"
