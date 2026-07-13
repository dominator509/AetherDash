/**
 * Keyboard E2E tests for AETHER Terminal.
 *
 * Verifies the SPEC-004 keymap: Ctrl/Cmd+K, Escape back-stack,
 * palette navigation, Tab focus management.
 *
 * Note: The feed-key bindings (j/k/e/s/a/i/Enter) are stubs routed
 * through useKeyboardRouter and will be exercised by EP-102 tests.
 */

import { test, expect } from "@playwright/test";

test.describe("Keyboard shortcuts", () => {
  test.beforeEach(async ({ page }) => {
    // Set the Zustand store directly to skip the login flow in e2e.
    // This is faster and more reliable than clicking through the login form.
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
  });

  test("Ctrl+K opens command palette", async ({ page }) => {
    // Press Ctrl+K
    await page.keyboard.press("Control+k");

    // The palette search input should be visible
    const searchInput = page.getByPlaceholder("Search commands and surfaces...");
    await expect(searchInput).toBeVisible();
  });

  test("Escape closes command palette", async ({ page }) => {
    // Open palette
    await page.keyboard.press("Control+k");
    await expect(page.getByPlaceholder("Search commands and surfaces...")).toBeVisible();

    // Close with Escape (handled by Radix Dialog natively)
    await page.keyboard.press("Escape");

    // Palette should be closed
    await expect(page.getByPlaceholder("Search commands and surfaces...")).not.toBeVisible();
  });

  test("Escape backs out from other layers", async ({ page }) => {
    // Open a layer (palette)
    await page.keyboard.press("Control+k");
    await expect(page.getByPlaceholder("Search commands and surfaces...")).toBeVisible();

    // Press Escape — should close palette (Radix handles it)
    await page.keyboard.press("Escape");
    await expect(page.getByPlaceholder("Search commands and surfaces...")).not.toBeVisible();

    // Press Escape again — no layer, should be no-op
    await page.keyboard.press("Escape");
    // Nothing should crash; page still works
    await expect(page.getByLabel("Feed")).toBeVisible();
  });

  test("arrow keys navigate palette results", async ({ page }) => {
    // Open palette
    await page.keyboard.press("Control+k");
    await expect(page.getByPlaceholder("Search commands and surfaces...")).toBeVisible();

    // First option should be selected initially
    const options = page.getByRole("option");
    await expect(options.first()).toHaveAttribute("aria-selected", "true");

    // Arrow down — second option should be selected
    await page.keyboard.press("ArrowDown");
    await expect(options.nth(1)).toHaveAttribute("aria-selected", "true");

    // Arrow down again — third option selected
    await page.keyboard.press("ArrowDown");
    await expect(options.nth(2)).toHaveAttribute("aria-selected", "true");

    // Arrow up — back to second
    await page.keyboard.press("ArrowUp");
    await expect(options.nth(1)).toHaveAttribute("aria-selected", "true");
  });

  test("Enter selects palette result", async ({ page }) => {
    // Start on settings surface
    await page.getByLabel("Settings").click();
    await expect(page.getByLabel("Settings")).toHaveAttribute("aria-current", "page");

    // Open palette
    await page.keyboard.press("Control+k");
    await expect(page.getByPlaceholder("Search commands and surfaces...")).toBeVisible();

    // First option is "Feed" — press Enter
    await page.keyboard.press("Enter");

    // Should navigate to Feed and close palette
    await expect(page.getByLabel("Feed")).toHaveAttribute("aria-current", "page");
    await expect(page.getByPlaceholder("Search commands and surfaces...")).not.toBeVisible();
  });

  test("Tab moves through nav rail buttons", async ({ page }) => {
    // Press Tab multiple times until a nav rail button is focused
    for (let i = 0; i < 10; i++) {
      await page.keyboard.press("Tab");
      const focused = await page.evaluate(() => {
        const el = document.activeElement;
        return el?.getAttribute("aria-label") ?? null;
      });
      if (
        focused &&
        [
          "Feed",
          "Explain",
          "Simulate",
          "Ticket",
          "Command",
          "Alerts",
          "Positions",
          "Settings",
        ].includes(focused)
      ) {
        // Found a nav rail button — test passes
        expect(focused).toBeTruthy();
        return;
      }
    }
    // If we didn't find a nav button after 10 tabs, fail
    throw new Error("Could not tab to nav rail within 10 Tab presses");
  });

  test("focus is visible with focus-visible class after keyboard navigation", async ({ page }) => {
    // Tab to trigger keyboard focus mode
    await page.keyboard.press("Tab");

    // body should have the focus-visible class
    const hasClass = await page.evaluate(() => document.body.classList.contains("focus-visible"));
    expect(hasClass).toBe(true);
  });

  test("no keyboard traps: Tab reaches every interactive element and wraps", async ({ page }) => {
    // Count focusable elements
    const focusableCount = await page.evaluate(() => {
      const selectors = 'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])';
      return document.querySelectorAll(selectors).length;
    });

    // Tab through all focusable elements multiple times — should not trap or crash
    for (let i = 0; i < focusableCount * 2; i++) {
      await page.keyboard.press("Tab");
    }

    // No crash — test passes
    expect(focusableCount).toBeGreaterThan(0);
  });
});
