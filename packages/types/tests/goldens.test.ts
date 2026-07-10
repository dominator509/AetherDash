import { describe, it, expect } from "vitest";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { canonicalJson } from "../src/index.js";

interface GoldenEntry { name: string; type: string; value: unknown; sha256: string; }

function loadGoldens(file: string): GoldenEntry[] {
  const path = `../../testdata/golden/core/${file}`;
  return JSON.parse(readFileSync(path, "utf-8"));
}

function sha256(data: string): string {
  return createHash("sha256").update(data).digest("hex");
}

const ALL_FILES = [
  "money", "market_key", "confidence", "edge", "quote", "order_book",
  "order_intent", "risk_verdict", "order", "fill", "position",
  "caps_snapshot", "market", "price_semantics", "opportunity",
  "audit_event", "error_envelope",
].map(f => `${f}.json`);

describe("Golden vectors — cross-language canonical bytes", () => {
  for (const file of ALL_FILES) {
    it(`${file}: all vectors verify SHA-256`, () => {
      const entries = loadGoldens(file);
      expect(entries.length).toBeGreaterThan(0);
      for (const entry of entries) {
        const canonical = canonicalJson(entry.value);
        const hash = sha256(canonical);
        expect(hash).toBe(entry.sha256);
      }
    });
  }
});
