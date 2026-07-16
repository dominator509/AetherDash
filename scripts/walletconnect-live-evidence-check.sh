#!/usr/bin/env bash
# Layer: 6 - Verification & Operations
# EP-306: Validate the operator-recorded WalletConnect live proof evidence file.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

if [ "$#" -ne 1 ]; then
  echo "usage: scripts/walletconnect-live-evidence-check.sh <evidence.json>"
  exit 2
fi

EVIDENCE_PATH="$1"
if [ ! -f "$EVIDENCE_PATH" ]; then
  echo "FAIL: evidence file not found: $EVIDENCE_PATH"
  exit 2
fi

python - "$EVIDENCE_PATH" <<'PY'
import json
import re
import sys
from pathlib import Path

path = Path(sys.argv[1])
try:
    evidence = json.loads(path.read_text(encoding="utf-8"))
except Exception as exc:
    print(f"FAIL: invalid JSON: {exc}")
    sys.exit(1)

required = [
    "command_timestamp_utc",
    "chain_id",
    "relay_url",
    "operator_account",
    "pairing_topic",
    "pairing_uri",
    "request_id",
    "request_method",
    "guardian_policy_state",
    "wallet_approved",
    "wallet_approval_artifact",
    "operator_recorded_by",
]

errors = []
for key in required:
    if key not in evidence:
        errors.append(f"missing required field: {key}")

if "relay_url" in evidence and not str(evidence["relay_url"]).startswith(("ws://", "wss://")):
    errors.append("relay_url must start with ws:// or wss://")

if "operator_account" in evidence:
    account = str(evidence["operator_account"])
    if not re.fullmatch(r"0x[0-9a-fA-F]{40}", account):
        errors.append("operator_account must be a 0x-prefixed 20-byte address")

if "pairing_uri" in evidence and not str(evidence["pairing_uri"]).startswith("wc:"):
    errors.append("pairing_uri must start with wc:")

if "pairing_topic" in evidence:
    topic = str(evidence["pairing_topic"])
    if not re.fullmatch(r"[0-9a-fA-F]{16,128}", topic):
        errors.append("pairing_topic must be hex-like and at least 16 chars")

if "request_method" in evidence and evidence["request_method"] != "eth_sendTransaction":
    errors.append("request_method must be eth_sendTransaction")

if "guardian_policy_state" in evidence and evidence["guardian_policy_state"] not in {"auto_approved", "approved"}:
    errors.append("guardian_policy_state must be auto_approved or approved")

if evidence.get("wallet_approved") is not True:
    errors.append("wallet_approved must be true")

artifact = str(evidence.get("wallet_approval_artifact", ""))
if not artifact.strip():
    errors.append("wallet_approval_artifact must not be empty")
elif not (
    re.fullmatch(r"0x[0-9a-fA-F]{64}", artifact)
    or artifact.startswith("wallet-confirmation:")
    or artifact.startswith("ops-log:")
):
    errors.append(
        "wallet_approval_artifact must be a 0x tx/hash, wallet-confirmation:<id>, or ops-log:<entry-id>"
    )

if "chain_id" in evidence:
    try:
        chain_id = int(evidence["chain_id"])
        if chain_id <= 0:
            errors.append("chain_id must be positive")
    except Exception:
        errors.append("chain_id must be an integer")

for forbidden in ("private_key", "seed_phrase", "mnemonic", "secret", "project_secret"):
    if forbidden in evidence:
        errors.append(f"forbidden secret-shaped field present: {forbidden}")

if errors:
    print("FAIL: WalletConnect evidence is incomplete")
    for error in errors:
        print(f"  - {error}")
    sys.exit(1)

print("walletconnect evidence: ok")
print(f"  evidence_file={path}")
print(f"  chain_id={evidence['chain_id']}")
print(f"  operator_account={str(evidence['operator_account']).lower()}")
print(f"  request_id={evidence['request_id']}")
print(f"  guardian_policy_state={evidence['guardian_policy_state']}")
PY
