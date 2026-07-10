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
  const path = `../../testdata/golden/core/${file}`;
  return JSON.parse(readFileSync(path, "utf-8"));
}

function sha256(data: string): string {
  return createHash("sha256").update(data).digest("hex");
}

describe("Golden vectors — cross-language canonical bytes", () => {
  const files = ["money.json", "edge.json"];

  for (const file of files) {
    it(`${file}: all vectors round-trip with matching SHA-256`, () => {
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
