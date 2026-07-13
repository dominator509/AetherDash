/**
 * Accessibility E2E tests for AETHER Terminal.
 *
 * Covers text scaling simulation, reduced motion, CSS variable presence
 * (contrast-related), focus ring visibility, and ARIA attribute checks.
 * This is NOT a full WCAG audit — axe-core / browser-level verification
 * should be added separately as the app matures.
 */

import { test, expect } from "@playwright/test";

test.describe("Accessibility", () => {
  test("text scaling simulation (200%) — no horizontal scroll", async ({ page }) => {
    await page.goto("/");

    // Simulate 200% text scaling by setting the root font-size via the DOM.
    // This is a text-scaling simulation, not a true browser zoom (which would
    // require a CDP Page.setDeviceMetricsScaleFactor call).
    await page.evaluate(() => {
      document.documentElement.style.fontSize = "200%";
    });

    // Verify no horizontal scroll occurs with scaled text.
    const noHorizontalScroll = await page.evaluate(() => {
      return document.documentElement.scrollWidth <= window.innerWidth;
    });

    expect(noHorizontalScroll).toBe(true);
  });

  test("reduced motion — animations are disabled", async ({ page }) => {
    // Emulate prefers-reduced-motion
    await page.emulateMedia({ reducedMotion: "reduce" });
    await page.goto("/");

    // The CSS media query in globals.css forces near-zero animation durations
    // when prefers-reduced-motion: reduce is active
    const duration = await page.evaluate(() => {
      const el = document.createElement("div");
      el.style.animation = "test 1s linear";
      document.body.appendChild(el);
      const duration = window.getComputedStyle(el).animationDuration;
      document.body.removeChild(el);
      return duration;
    });

    // With reduced motion, animation should be near-zero duration
    // The CSS sets 0.01ms which browsers may report as "1e-05s" (scientific notation)
    expect(
      duration === "0.01ms" || duration === "0s" || duration === "0ms" || duration === "1e-05s",
    ).toBe(true);
  });

  test("CSS variable verification — contrast-related custom properties", async ({ page }) => {
    await page.goto("/");

    // Verify that key contrast-related CSS custom properties are defined on
    // :root. This is a presence check, not a WCAG contrast ratio measurement.
    const cssVars = await page.evaluate(() => {
      const style = getComputedStyle(document.documentElement);
      return {
        focusRingColor: style.getPropertyValue("--tw-ring-color").trim(),
        bodyBg: style.getPropertyValue("--body-bg").trim(),
        textColor: style.getPropertyValue("--text-color").trim(),
      };
    });

    // The focus ring color should be a non-empty string, indicating the
    // theme provides a visible focus indicator.
    expect(cssVars.focusRingColor).toBeTruthy();
  });

  test("key ARIA attributes are present and correct", async ({ page }) => {
    // Set Zustand store directly to skip login flow and render the shell
    await page.goto("/");
    await page.evaluate(() => {
      const store = (window as Record<string, unknown>).__AETHER_STORE__ as {
        setState: (s: Record<string, unknown>) => void;
      };
      if (store) {
        store.setState({ authenticated: true, connectionStatus: "connected" });
      }
    });
    await expect(page.getByLabel("Feed")).toBeVisible({ timeout: 5000 });

    // Verify the main content landmark exists
    const main = page.getByRole("main");
    await expect(main).toBeVisible();

    // Verify navigation landmark exists (nav element)
    const nav = page.locator("nav");
    await expect(nav).toBeVisible();

    // Mode toggle group has aria-label
    const modeToggle = page.getByLabel("Display mode");
    await expect(modeToggle).toBeVisible();

    // Nav surface buttons all have aria-label set correctly
    const surfaces = [
      "Feed",
      "Explain",
      "Simulate",
      "Ticket",
      "Command",
      "Alerts",
      "Positions",
      "Settings",
    ];
    for (const surface of surfaces) {
      const btn = page.getByLabel(surface);
      await expect(btn).toBeVisible();
      await expect(btn).toHaveAttribute("aria-label", surface);
    }
  });
});
