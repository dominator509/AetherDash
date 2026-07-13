/**
 * Shell E2E tests for AETHER Terminal.
 *
 * Verifies that the basic shell layout renders and surface navigation works.
 */

import { test, expect } from "@playwright/test";

test.describe("App shell", () => {
  test.beforeEach(async ({ page }) => {
    // Set the Zustand store directly to skip the login flow in e2e.
    // This is faster and more reliable than clicking through the login form,
    // and avoids the dependency on a running gateway.
    await page.goto("/");
    await page.evaluate(() => {
      const store = (window as Record<string, unknown>).__AETHER_STORE__ as {
        setState: (s: Record<string, unknown>) => void;
        getState: () => Record<string, unknown>;
      };
      if (store) {
        store.setState({ authenticated: true });
      }
    });
    await expect(page.getByLabel("Feed")).toBeVisible({ timeout: 5000 });
  });

  test("app loads without error", async () => {
    // Shell is visible after successful auth (handled by beforeEach)
    // Test passes if beforeEach completes without error
  });

  test("status bar shows connected state after authentication", async ({ page }) => {
    // Set connection status to connected since there's no real gateway in e2e
    await page.evaluate(() => {
      const store = (window as Record<string, unknown>).__AETHER_STORE__ as {
        setState: (s: Record<string, unknown>) => void;
      };
      store?.setState?.({ connectionStatus: "connected" });
    });
    await expect(page.getByText("Connected")).toBeVisible();
  });

  test("NavRail renders all 8 surface buttons", async ({ page }) => {
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
      await expect(page.getByLabel(surface)).toBeVisible();
    }
  });

  test("clicking a surface button changes active surface", async ({ page }) => {
    // Click Settings — the last surface in the nav rail
    await page.getByLabel("Settings").click();
    // The Settings surface should now be active
    await expect(page.getByLabel("Settings")).toHaveAttribute("aria-current", "page");

    // Click Feed — should become active
    await page.getByLabel("Feed").click();
    await expect(page.getByLabel("Feed")).toHaveAttribute("aria-current", "page");
    // Settings should no longer be active
    await expect(page.getByLabel("Settings")).not.toHaveAttribute("aria-current", "page");
  });
});
