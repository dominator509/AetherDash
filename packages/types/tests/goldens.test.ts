import { describe, it, expect } from "vitest";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { validateAndCanonicalize } from "../src/index.js";

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
  // P1-7: Adversarial canonical vectors (cross-language)
  "unicode",
  "ordering",
  "null_omission",
  "empty_collections",
].map((f) => `${f}.json`);

describe("Typed golden vectors — cross-language canonical bytes", () => {
  for (const file of FILES) {
    it(`${file}: validate, canonicalize, verify SHA-256`, () => {
      const entries = loadGoldens(file);
      expect(entries.length).toBeGreaterThan(0);
      for (const entry of entries) {
        // validateAndCanonicalize validates the value against the type schema,
        // then returns the deterministic canonical JSON string.
        // This ensures we hash the validated/re-serialized output (not the raw JSON),
        // matching Rust's serde_json round-trip.
        const canonical = validateAndCanonicalize(entry.type, entry.value);
        const hash = sha256(canonical);
        expect(hash).toBe(entry.sha256);
      }
    });
  }
});
