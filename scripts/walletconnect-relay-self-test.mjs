#!/usr/bin/env node
// Controlled public-relay diagnostic for EP-306 M6. This proves WalletConnect
// pairing/session/request delivery only; it is not operator-wallet evidence and
// must never be accepted by walletconnect-live-evidence-check.sh.

import { randomUUID } from "node:crypto";
import WebSocket from "ws";

globalThis.WebSocket = WebSocket;
const { Core } = await import("@walletconnect/core");
const { SignClient } = await import("@walletconnect/sign-client");
const { WalletKit } = await import("@reown/walletkit");

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
const caipAccount = `${caipChain}:${operatorAccount}`;
const methods = ["eth_sendTransaction"];
const events = ["accountsChanged", "chainChanged"];
const timeoutMs = 45_000;

const withTimeout = (promise, label) => {
  let timer;
  const timeout = new Promise((_, reject) => {
    timer = setTimeout(
      () => reject(new Error(`${label} timed out after ${timeoutMs}ms`)),
      timeoutMs,
    );
  });
  return Promise.race([promise, timeout]).finally(() => clearTimeout(timer));
};

const walletCore = new Core({
  projectId,
  relayUrl,
  storageOptions: { database: ":memory:" },
  customStoragePrefix: `aether-relay-wallet-${randomUUID()}`,
});
const wallet = await WalletKit.init({
  core: walletCore,
  metadata: {
    name: "AetherDash Relay Diagnostic Wallet",
    description: "Non-signing WalletConnect transport test peer",
    url: "https://github.com/dominator509/AetherDash",
    icons: ["https://walletconnect.com/walletconnect-logo.png"],
  },
});

const dapp = await SignClient.init({
  projectId,
  relayUrl,
  storageOptions: { database: ":memory:" },
  customStoragePrefix: `aether-relay-dapp-${randomUUID()}`,
  metadata: {
    name: "AetherDash Wallet Guardian Relay Diagnostic",
    description: "WalletConnect transport self-test",
    url: "https://github.com/dominator509/AetherDash",
    icons: ["https://walletconnect.com/walletconnect-logo.png"],
  },
});

let proposalResolve;
let proposalReject;
const proposalHandled = new Promise((resolve, reject) => {
  proposalResolve = resolve;
  proposalReject = reject;
});
wallet.on("session_proposal", async ({ id }) => {
  try {
    const session = await wallet.approveSession({
      id,
      namespaces: {
        eip155: {
          accounts: [caipAccount],
          methods,
          events,
        },
      },
    });
    proposalResolve(session);
  } catch (error) {
    proposalReject(error);
  }
});

const diagnosticHash = `0x${"ab".repeat(32)}`;
wallet.on("session_request", async ({ topic, id }) => {
  await wallet.respondSessionRequest({
    topic,
    response: { jsonrpc: "2.0", id, result: diagnosticHash },
  });
});

const { uri, approval } = await dapp.connect({
  requiredNamespaces: {
    eip155: { chains: [caipChain], methods, events },
  },
});
if (!uri) throw new Error("diagnostic dapp did not create a pairing URI");

await wallet.pair({ uri });
const [dappSession, walletSession] = await Promise.all([
  withTimeout(approval(), "dapp session approval"),
  withTimeout(proposalHandled, "wallet session proposal"),
]);
if (dappSession.topic !== walletSession.topic) {
  throw new Error("dapp and wallet resolved different session topics");
}

const result = await withTimeout(
  dapp.request({
    topic: dappSession.topic,
    chainId: caipChain,
    request: {
      method: "eth_sendTransaction",
      params: [
        {
          from: operatorAccount,
          to: operatorAccount,
          value: "0x0",
          data: "0x",
          gas: "0x5208",
        },
      ],
    },
  }),
  "session request response",
);
if (result !== diagnosticHash) throw new Error("diagnostic response did not round-trip");

console.log("walletconnect relay self-test: ok");
console.log(`  relay_url=${relayUrl}`);
console.log(`  chain=${caipChain}`);
console.log(`  session_topic=${dappSession.topic}`);
console.log("  scope=pairing/session/request transport only (not M6 wallet evidence)");

await dapp.disconnect({
  topic: dappSession.topic,
  reason: { code: 6000, message: "relay diagnostic complete" },
});
await Promise.allSettled([
  dapp.core.relayer.transportClose(),
  wallet.core.relayer.transportClose(),
]);
