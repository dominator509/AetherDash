#!/usr/bin/env bash
# Layer: 6 - Verification & Operations
# EP-306: WalletConnect live relay/session proof runner.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

required=(
  AETHER_GUARDIAN__WC_PROJECT_ID
  AETHER_GUARDIAN__WC_RELAY_URL
  AETHER_GUARDIAN__WC_OPERATOR_ACCOUNT
  AETHER_GUARDIAN__WC_TESTNET_CHAIN_ID
)

missing=()
for name in "${required[@]}"; do
  if [ -z "${!name:-}" ]; then
    missing+=("$name")
  fi
done

if [ "${#missing[@]}" -gt 0 ]; then
  echo "MISSING WalletConnect live proof env:"
  for name in "${missing[@]}"; do
    echo "  - $name"
  done
  echo
  echo "Set these from the operator-controlled WalletConnect project and testnet wallet session,"
  echo "then rerun: scripts/walletconnect-live-readiness.sh"
  exit 2
fi

case "${AETHER_GUARDIAN__WC_RELAY_URL}" in
  ws://*|wss://*) ;;
  *)
    echo "FAIL: AETHER_GUARDIAN__WC_RELAY_URL must start with ws:// or wss://"
    exit 2
    ;;
esac

case "${AETHER_GUARDIAN__WC_OPERATOR_ACCOUNT}" in
  0x????????????????????????????????????????) ;;
  *)
    echo "FAIL: AETHER_GUARDIAN__WC_OPERATOR_ACCOUNT must be a 0x-prefixed 20-byte address"
    exit 2
    ;;
esac

echo "WalletConnect live relay/session env present:"
echo "  AETHER_GUARDIAN__WC_PROJECT_ID=<set>"
echo "  AETHER_GUARDIAN__WC_RELAY_URL=<set>"
echo "  AETHER_GUARDIAN__WC_OPERATOR_ACCOUNT=${AETHER_GUARDIAN__WC_OPERATOR_ACCOUNT}"
echo "  AETHER_GUARDIAN__WC_TESTNET_CHAIN_ID=${AETHER_GUARDIAN__WC_TESTNET_CHAIN_ID}"
echo
echo "Starting the real WalletConnect relay/session client..."
node scripts/walletconnect-live-client.mjs

evidence_path="${AETHER_GUARDIAN__WC_EVIDENCE_PATH:-data/walletconnect-live-evidence.json}"
scripts/walletconnect-live-evidence-check.sh "$evidence_path"
