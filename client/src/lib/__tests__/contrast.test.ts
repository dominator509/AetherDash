/**
 * Tests for contrast ratio utilities.
 *
 * Verifies WCAG AA compliance for the actual theme color pairs
 * defined in theme.css.
 *
 * @see https://www.w3.org/TR/WCAG21/#contrast-minimum
 */

import { describe, it, expect } from "vitest";
import { getContrastRatio, meetsAA, meetsAAA, relativeLuminance } from "../contrast";

// ── Known test vectors ──────────────────────────────────────────────────────

describe("relativeLuminance", () => {
  it("black is 0", () => {
    expect(relativeLuminance("#000000")).toBeCloseTo(0, 4);
  });

  it("white is 1", () => {
    expect(relativeLuminance("#ffffff")).toBeCloseTo(1, 4);
  });

  it("handles 3-digit hex", () => {
    expect(relativeLuminance("#fff")).toBeCloseTo(1, 4);
    expect(relativeLuminance("#000")).toBeCloseTo(0, 4);
  });

  it("handles 8-digit hex (ignores alpha)", () => {
    expect(relativeLuminance("#ffffff80")).toBeCloseTo(1, 4);
  });

  it("throws on malformed hex", () => {
    expect(() => relativeLuminance("#xyz")).toThrow();
    expect(() => relativeLuminance("not-a-color")).toThrow();
  });
});

// ── Contrast ratio known pairs ──────────────────────────────────────────────

describe("getContrastRatio", () => {
  it("white on black is 21:1", () => {
    const ratio = getContrastRatio("#ffffff", "#000000");
    expect(ratio).toBeCloseTo(21, 0);
  });

  it("black on white is 21:1 (order invariant)", () => {
    const ratio = getContrastRatio("#000000", "#ffffff");
    expect(ratio).toBeCloseTo(21, 0);
  });

  it("same color is 1:1", () => {
    const ratio = getContrastRatio("#777777", "#777777");
    expect(ratio).toBeCloseTo(1, 1);
  });

  it("#777777 on #FFFFFF fails AA", () => {
    const ratio = getContrastRatio("#777777", "#ffffff");
    // #777777 on white is approximately 4.1:1 — below 4.5:1
    expect(ratio).toBeLessThan(4.5);
    expect(meetsAA(ratio)).toBe(false);
  });

  it("#777777 on #FFFFFF passes AA for large text", () => {
    const ratio = getContrastRatio("#777777", "#ffffff");
    // ~4.1:1 — passes 3:1 threshold for large text
    expect(meetsAA(ratio, true)).toBe(true);
  });

  it("#123456 on #ccddee — contrast computed correctly", () => {
    // Dark blue on light blue-gray — expect high contrast (> 5:1)
    const ratio = getContrastRatio("#123456", "#ccddee");
    expect(ratio).toBeGreaterThan(5);
    expect(ratio).toBeLessThan(12);
  });

  it("#000000 on #ffff00 — black on yellow is very high contrast", () => {
    // Black on pure yellow should be > 15:1
    const ratio = getContrastRatio("#000000", "#ffff00");
    expect(ratio).toBeGreaterThan(15);
    expect(ratio).toBeLessThan(21);
  });
});

// ── meetsAA / meetsAAA ──────────────────────────────────────────────────────

describe("meetsAA", () => {
  it("4.5:1 passes for normal text", () => {
    expect(meetsAA(4.5)).toBe(true);
  });

  it("4.49:1 fails for normal text", () => {
    expect(meetsAA(4.49)).toBe(false);
  });

  it("3:1 passes for large text", () => {
    expect(meetsAA(3, true)).toBe(true);
  });

  it("2.99:1 fails for large text", () => {
    expect(meetsAA(2.99, true)).toBe(false);
  });
});

describe("meetsAAA", () => {
  it("7:1 passes for normal text", () => {
    expect(meetsAAA(7)).toBe(true);
  });

  it("6.99:1 fails for normal text", () => {
    expect(meetsAAA(6.99)).toBe(false);
  });

  it("4.5:1 passes for large text", () => {
    expect(meetsAAA(4.5, true)).toBe(true);
  });

  it("4.49:1 fails for large text", () => {
    expect(meetsAAA(4.49, true)).toBe(false);
  });
});

// ── Dark theme contrast verification — actual values from theme.css ─────────

describe("dark theme contrast (against #030712)", () => {
  const BG = "#030712";

  it("text-primary (#f9fafb) on bg-primary meets AA", () => {
    const r = getContrastRatio("#f9fafb", BG);
    expect(r).toBeGreaterThanOrEqual(4.5);
  });

  it("text-secondary (#9ca3af) on bg-primary meets AA", () => {
    const r = getContrastRatio("#9ca3af", BG);
    expect(r).toBeGreaterThanOrEqual(4.5);
  });

  it("text-muted (#78828f) on bg-primary meets AA", () => {
    const r = getContrastRatio("#78828f", BG);
    expect(r).toBeGreaterThanOrEqual(4.5);
  });

  it("accent (#3b82f6) on bg-primary meets AA large-text minimum (≥ 3:1)", () => {
    const r = getContrastRatio("#3b82f6", BG);
    // Accent is used for buttons and interactive elements — large text minimum
    expect(r).toBeGreaterThanOrEqual(3);
  });

  it("success (#22c55e) on bg-primary meets AA", () => {
    const r = getContrastRatio("#22c55e", BG);
    expect(r).toBeGreaterThanOrEqual(4.5);
  });

  it("warning (#f59e0b) on bg-primary meets AA", () => {
    const r = getContrastRatio("#f59e0b", BG);
    expect(r).toBeGreaterThanOrEqual(4.5);
  });

  it("error (#ef4444) on bg-primary meets AA", () => {
    const r = getContrastRatio("#ef4444", BG);
    expect(r).toBeGreaterThanOrEqual(4.5);
  });
});

// ── Light theme contrast verification ───────────────────────────────────────

describe("light theme contrast (against #ffffff)", () => {
  const BG = "#ffffff";

  it("text-primary (#111827) on bg-primary meets AA", () => {
    const r = getContrastRatio("#111827", BG);
    expect(r).toBeGreaterThanOrEqual(4.5);
  });

  it("text-secondary (#6b7280) on bg-primary meets AA", () => {
    const r = getContrastRatio("#6b7280", BG);
    expect(r).toBeGreaterThanOrEqual(4.5);
  });

  it("accent (#2563eb) on bg-primary meets AA", () => {
    const r = getContrastRatio("#2563eb", BG);
    expect(r).toBeGreaterThanOrEqual(4.5);
  });

  it("success (#15803d) on bg-primary meets AA", () => {
    const r = getContrastRatio("#15803d", BG);
    expect(r).toBeGreaterThanOrEqual(4.5);
  });

  it("error (#dc2626) on bg-primary meets AA", () => {
    const r = getContrastRatio("#dc2626", BG);
    expect(r).toBeGreaterThanOrEqual(4.5);
  });
});

// ── Focus ring contrast ─────────────────────────────────────────────────────

describe("focus ring contrast", () => {
  it("blue-500 (#3b82f6) on dark bg (#030712) meets 3:1 minimum", () => {
    const r = getContrastRatio("#3b82f6", "#030712");
    expect(r).toBeGreaterThanOrEqual(3);
  });

  it("blue-500 (#3b82f6) on light bg (#ffffff) meets 3:1 minimum", () => {
    const r = getContrastRatio("#3b82f6", "#ffffff");
    expect(r).toBeGreaterThanOrEqual(3);
  });
});
