/**
 * Contrast ratio utilities for WCAG AA compliance verification.
 *
 * Computes relative luminance from sRGB hex values and determines
 * whether contrast ratios meet WCAG AA thresholds.
 *
 * @see https://www.w3.org/TR/WCAG21/#contrast-minimum
 */

// ── sRGB relative luminance ──────────────────────────────────────────────────

/**
 * Linearize a single sRGB channel (0–1 range).
 * Using the sRGB transfer function from IEC 61966-2-1.
 */
function linearize(channel: number): number {
  if (channel <= 0.04045) {
    return channel / 12.92;
  }
  return Math.pow((channel + 0.055) / 1.055, 2.4);
}

/**
 * Compute relative luminance from an sRGB hex color string.
 * Accepts `#rgb`, `#rrggbb`, `#rrggbbaa` formats.
 *
 * Returns a number in the range [0, 1] where 0 = black, 1 = white.
 *
 * @throws If the hex string is malformed.
 */
export function relativeLuminance(hex: string): number {
  const raw = hex.replace("#", "");

  let r: number;
  let g: number;
  let b: number;

  // Validate hex characters
  if (!/^[0-9a-fA-F]+$/.test(raw)) {
    throw new Error(`Invalid hex color: ${hex} — contains non-hex characters`);
  }

  if (raw.length === 3) {
    r = parseInt(raw.charAt(0) + raw.charAt(0), 16) / 255;
    g = parseInt(raw.charAt(1) + raw.charAt(1), 16) / 255;
    b = parseInt(raw.charAt(2) + raw.charAt(2), 16) / 255;
  } else if (raw.length === 6 || raw.length === 8) {
    r = parseInt(raw.substring(0, 2), 16) / 255;
    g = parseInt(raw.substring(2, 4), 16) / 255;
    b = parseInt(raw.substring(4, 6), 16) / 255;
  } else {
    throw new Error(`Invalid hex color: ${hex} — unexpected length ${raw.length}`);
  }

  const R = linearize(r);
  const G = linearize(g);
  const B = linearize(b);

  // Weighted sum per ITU-R BT.709
  return 0.2126 * R + 0.7152 * G + 0.0722 * B;
}

// ── Contrast ratio ───────────────────────────────────────────────────────────

/**
 * Compute the contrast ratio between two sRGB hex colors.
 *
 * Ratio is (L1 + 0.05) / (L2 + 0.05), where L1 >= L2.
 * Returns a value in the range [1, 21].
 */
export function getContrastRatio(hex1: string, hex2: string): number {
  const l1 = relativeLuminance(hex1);
  const l2 = relativeLuminance(hex2);

  const lighter = Math.max(l1, l2);
  const darker = Math.min(l1, l2);

  return (lighter + 0.05) / (darker + 0.05);
}

// ── WCAG AA threshold ────────────────────────────────────────────────────────

/**
 * Check whether a given contrast ratio meets the WCAG AA threshold.
 *
 * - Normal text (default): ≥ 4.5:1
 * - Large text (≥ 18px or ≥ 14px bold): ≥ 3:1
 *
 * @returns `true` if the ratio meets AA requirements.
 */
export function meetsAA(ratio: number, isLargeText = false): boolean {
  return ratio >= (isLargeText ? 3 : 4.5);
}

// ── WCAG AAA threshold (bonus) ───────────────────────────────────────────────

/**
 * Check whether a given contrast ratio meets the WCAG AAA threshold.
 *
 * - Normal text: ≥ 7:1
 * - Large text: ≥ 4.5:1
 */
export function meetsAAA(ratio: number, isLargeText = false): boolean {
  return ratio >= (isLargeText ? 4.5 : 7);
}
