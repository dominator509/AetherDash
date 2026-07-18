#!/usr/bin/env node
// Real WalletConnect v2 relay/session transport for EP-306 M6.
// The Rust Guardian evaluates policy and emits the exact request only after the
// wallet session has granted the configured account and chain.

import { spawnSync } from "node:child_process";
import { createHash, randomUUID } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import QRCode from "qrcode";
import qrcode from "qrcode-terminal";
import WebSocket from "ws";

// Sign Client 2.19 expects the Node websocket to expose terminate(). Node 24's
// built-in WHATWG WebSocket does not, so install the standard Node adapter
// before dynamically loading WalletConnect.
globalThis.WebSocket = WebSocket;
const { SignClient } = await import("@walletconnect/sign-client");

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");

const requiredEnv = (name) => {
  const value = process.env[name]?.trim();
  if (!value) throw new Error(`missing ${name}`);
  return value;
};

const projectId = requiredEnv("AETHER_GUARDIAN__WC_PROJECT_ID");
const relayUrl = requiredEnv("AETHER_GUARDIAN__WC_RELAY_URL");
const operatorAccount = requiredEnv("AETHER_GUARDIAN__WC_OPERATOR_ACCOUNT").toLowerCase();
const chainId = Number(requiredEnv("AETHER_GUARDIAN__WC_TESTNET_CHAIN_ID"));
const caipChain = `eip155:${chainId}`;
const evidencePath = resolve(
  root,
  process.env.AETHER_GUARDIAN__WC_EVIDENCE_PATH ?? "data/walletconnect-live-evidence.json",
);

if (!/^0x[0-9a-f]{40}$/.test(operatorAccount)) {
  throw new Error("operator account must be a 0x-prefixed 20-byte address");
}
if (!Number.isSafeInteger(chainId) || chainId <= 0) {
  throw new Error("testnet chain id must be a positive safe integer");
}
if (!/^wss?:\/\//.test(relayUrl)) {
  throw new Error("relay URL must use ws:// or wss://");
}

const client = await SignClient.init({
  projectId,
  relayUrl,
  storageOptions: { database: ":memory:" },
  customStoragePrefix: `aether-m6-${randomUUID()}`,
  metadata: {
    name: "AetherDash Wallet Guardian",
    description: "Policy-gated external transaction signing",
    url: "https://github.com/dominator509/AetherDash",
    icons: ["https://walletconnect.com/walletconnect-logo.png"],
  },
});

const { uri, approval } = await client.connect({
  // Keep Sepolia and eth_sendTransaction mandatory. Sign Client is pinned to
  // the last interoperable release that preserves this required wire shape;
  // newer CAIP-25 normalization caused multiple mobile wallets to stall.
  requiredNamespaces: {
    eip155: {
      chains: [caipChain],
      methods: ["eth_sendTransaction"],
      events: ["accountsChanged", "chainChanged"],
    },
  },
});
if (!uri) throw new Error("WalletConnect did not return a pairing URI");

const pairingTopic = uri.match(/^wc:([0-9a-f]+)@2(?:\?|$)/i)?.[1];
if (!pairingTopic) throw new Error("WalletConnect returned an invalid pairing URI");

// Topic-stamp the filename so desktop renderers and chat clients cannot reuse
// a cached QR from an earlier five-minute pairing window.
const pairingImagePath = resolve(
  root,
  `data/walletconnect-pairing-${pairingTopic.slice(0, 16)}.png`,
);
mkdirSync(dirname(pairingImagePath), { recursive: true });
await QRCode.toFile(pairingImagePath, uri, {
  errorCorrectionLevel: "M",
  margin: 4,
  width: 512,
});
console.log(`Pairing QR image: ${pairingImagePath}`);
console.log("\nScan this QR code with the configured operator wallet:\n");
qrcode.generate(uri, { small: true });
console.log(`\nPairing topic: ${pairingTopic}`);
console.log(
  "The raw pairing URI is intentionally not printed because it contains the pairing key.",
);
console.log("Waiting for the operator wallet to approve the session...");

const expiryTimestamp = Number(
  new URLSearchParams(uri.slice(uri.indexOf("?") + 1)).get("expiryTimestamp"),
);
const pairingTimeoutMs = Math.max(
  1,
  (Number.isSafeInteger(expiryTimestamp) ? expiryTimestamp * 1000 : Date.now() + 5 * 60_000) -
    Date.now(),
);
let pairingTimer;
const pairingExpired = new Promise((_, reject) => {
  pairingTimer = setTimeout(
    () => reject(new Error("WalletConnect pairing URI expired before approval")),
    pairingTimeoutMs,
  );
});
const session = await Promise.race([approval(), pairingExpired]).finally(() =>
  clearTimeout(pairingTimer),
);
const grantedAccount = session.namespaces.eip155?.accounts.find((account) => {
  const [namespace, grantedChain, address] = account.split(":");
  return (
    namespace === "eip155" &&
    grantedChain === String(chainId) &&
    address?.toLowerCase() === operatorAccount
  );
});
if (!grantedAccount) {
  throw new Error(`wallet session did not grant ${caipChain}:${operatorAccount}`);
}

console.log("Session approved. Evaluating the exact transaction in the Guardian...");
const guardian = spawnSync(
  "cargo",
  ["run", "--quiet", "-p", "aether-wallet-guardian", "--example", "wc_policy_packet"],
  {
    cwd: root,
    encoding: "utf8",
    env: process.env,
    windowsHide: true,
  },
);
if (guardian.status !== 0) {
  throw new Error(`Guardian policy evaluation failed: ${guardian.stderr || guardian.stdout}`);
}
const packet = JSON.parse(guardian.stdout);
if (
  !["approved", "auto_approved"].includes(packet.guardian_policy_state) ||
  packet.chain_id !== chainId ||
  packet.operator_account !== operatorAccount ||
  packet.request?.method !== "eth_sendTransaction" ||
  packet.request?.params?.[0]?.from?.toLowerCase() !== operatorAccount
) {
  throw new Error("Guardian policy packet does not match the approved session");
}

console.log("Guardian policy approved. Review and approve the transaction in the wallet.");
const result = await client.request({
  topic: session.topic,
  chainId: caipChain,
  request: packet.request,
});
if (typeof result !== "string" || !/^0x[0-9a-f]{64}$/i.test(result)) {
  throw new Error(`wallet returned an invalid transaction hash: ${String(result)}`);
}

const evidence = {
  command_timestamp_utc: new Date().toISOString(),
  chain_id: chainId,
  relay_url: relayUrl,
  operator_account: operatorAccount,
  pairing_topic: pairingTopic,
  pairing_uri_sha256: createHash("sha256").update(uri).digest("hex"),
  session_topic: session.topic,
  proposal_id: packet.proposal_id,
  proposal_hash: packet.proposal_hash,
  policy_trace: packet.policy_trace,
  request_id: packet.request.id,
  request_method: packet.request.method,
  guardian_policy_state: packet.guardian_policy_state,
  wallet_approved: true,
  wallet_approval_artifact: result,
  operator_recorded_by: "walletconnect-live-client",
};
mkdirSync(dirname(evidencePath), { recursive: true });
writeFileSync(evidencePath, `${JSON.stringify(evidence, null, 2)}\n`, {
  encoding: "utf8",
  mode: 0o600,
});
console.log(`Wallet approval received: ${result}`);
console.log(`Evidence written to: ${evidencePath}`);

try {
  await client.disconnect({
    topic: session.topic,
    reason: { code: 6000, message: "EP-306 M6 proof complete" },
  });
} catch (error) {
  console.warn(`WalletConnect session cleanup warning: ${error.message}`);
}
await client.core.relayer.transportClose();
