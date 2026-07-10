import { describe, it, expect } from "vitest";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { canonicalJson } from "../src/index.js";

interface GoldenEntry {
  name: string;
  type: string;
  value: unknown;
  sha256: string;
}

function loadGoldens(file: string): GoldenEntry[] {
  return JSON.parse(readFileSync(`../../testdata/golden/core/${file}`, "utf-8"));
}

function sha256(data: string): string {
  return createHash("sha256").update(data).digest("hex");
}

const TYPE_VALIDATORS: Record<string, (v: unknown) => string> = {
  Money: (v) => {
    const m = v as Record<string, unknown>;
    if (typeof m.amount !== "string" || typeof m.currency !== "string")
      throw new Error("Money: invalid");
    return "";
  },
  MarketKey: (v) => {
    const s = v as string;
    if (!s.startsWith("mkt:") || s.split(":").length < 3)
      throw new Error("MarketKey: invalid format");
    return "";
  },
  Confidence: (v) => {
    const s = v as string;
    const n = parseFloat(s);
    if (isNaN(n) || n < 0 || n > 1) throw new Error("Confidence: out of range");
    return "";
  },
  EdgeDecomposition: (v) => {
    const e = v as Record<string, unknown>;
    const g = Number(e.gross_spread);
    const net = Number(e.net_edge);
    const costs = [
      "fees",
      "slippage_est",
      "funding_cost",
      "gas_cost",
      "bridge_cost",
      "settlement_mismatch_discount",
      "liquidity_haircut",
      "staleness_penalty",
      "confidence_penalty",
    ].reduce((s, k) => s + Number(e[k] ?? 0), 0);
    if (Math.abs(net - (g - costs)) > 0.0001)
      throw new Error("EdgeDecomposition: sum law violation");
    return "";
  },
  Quote: () => "",
  OrderBook: () => "",
  OrderIntent: () => "",
  RiskVerdict: () => "",
  Order: () => "",
  Fill: () => "",
  Position: () => "",
  CapsSnapshot: () => "",
  Market: () => "",
  PriceSemantics: () => "",
  Opportunity: () => "",
  AuditEvent: () => "",
  ErrorEnvelope: () => "",
};

const FILES = [
  "money",
  "market_key",
  "confidence",
  "edge",
  "quote",
  "order_book",
  "order_intent",
  "risk_verdict",
  "order",
  "fill",
  "position",
  "caps_snapshot",
  "market",
  "price_semantics",
  "opportunity",
  "audit_event",
  "error_envelope",
].map((f) => `${f}.json`);

describe("Typed golden vectors — cross-language canonical bytes", () => {
  for (const file of FILES) {
    it(`${file}: deserialize, validate, re-serialize, verify SHA-256`, () => {
      const entries = loadGoldens(file);
      expect(entries.length).toBeGreaterThan(0);
      for (const entry of entries) {
        // Validate type label is known
        const validator = TYPE_VALIDATORS[entry.type];
        expect(validator, `Unknown type: ${entry.type}`).toBeDefined();
        // Run type-specific validation (throws on invariant violation)
        validator(entry.value);
        // Re-serialize canonically and verify hash
        const canonical = canonicalJson(entry.value);
        const hash = sha256(canonical);
        expect(hash).toBe(entry.sha256);
      }
    });
  }
});
