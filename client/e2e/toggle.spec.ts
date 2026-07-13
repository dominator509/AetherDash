/**
 * Mode toggle E2E tests for AETHER Terminal.
 *
 * Verifies Simple/Advanced mode switching, persistence, and
 * the Ctrl/Cmd+. keyboard shortcut (handled by useKeyboardRouter
 * in keyboard.ts, not by the ModeToggle component).
 */

import { test, expect } from "@playwright/test";

test.describe("Mode toggle", () => {
  test.beforeEach(async ({ page }) => {
    // Set the Zustand store directly to skip the login flow in e2e.
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

  test("toggle switches between Simple and Advanced", async ({ page }) => {
    // Default is Simple mode — find the Simple button
    const simple = page.getByLabel("Simple mode");
    const advanced = page.getByLabel("Advanced mode");

    // Simple should be checked initially
    await expect(simple).toHaveAttribute("aria-checked", "true");
    await expect(advanced).toHaveAttribute("aria-checked", "false");

    // Click Advanced
    await advanced.click();
    await expect(simple).toHaveAttribute("aria-checked", "false");
    await expect(advanced).toHaveAttribute("aria-checked", "true");

    // Click Simple to go back
    await simple.click();
    await expect(simple).toHaveAttribute("aria-checked", "true");
    await expect(advanced).toHaveAttribute("aria-checked", "false");
  });

  test("mode persists after surface change", async ({ page }) => {
    // Switch to Advanced mode
    await page.getByLabel("Advanced mode").click();
    await expect(page.getByLabel("Advanced mode")).toHaveAttribute("aria-checked", "true");

    // Change surface
    await page.getByLabel("Explain").click();
    await expect(page.getByLabel("Explain")).toHaveAttribute("aria-current", "page");

    // Mode should still be Advanced
    await expect(page.getByLabel("Advanced mode")).toHaveAttribute("aria-checked", "true");
  });

  test("Ctrl+. toggles mode", async ({ page }) => {
    // The keyboard router only accepts Ctrl+. when connectionStatus === "connected".
    // Set the store state directly since there's no real gateway in e2e.
    await page.evaluate(() => {
      const store = (window as Record<string, unknown>).__AETHER_STORE__ as {
        setState: (s: Record<string, unknown>) => void;
      };
      if (store?.setState) {
        store.setState({ connectionStatus: "connected" });
      }
    });

    // Default is Simple
    await expect(page.getByLabel("Simple mode")).toHaveAttribute("aria-checked", "true");

    // Ctrl+. toggles to Advanced
    await page.keyboard.press("Control+.");
    await expect(page.getByLabel("Advanced mode")).toHaveAttribute("aria-checked", "true");

    // Ctrl+. toggles back to Simple
    await page.keyboard.press("Control+.");
    await expect(page.getByLabel("Simple mode")).toHaveAttribute("aria-checked", "true");
  });

  test("Simple mode shows centered layout, Advanced shows full-width", async ({ page }) => {
    // In Simple mode the flex container that wraps the main content has justify-center.
    // DOM structure (simple mode):
    //   div.flex.flex-1.overflow-auto.justify-center  <-- container with justify-center
    //     div.w-full.max-w-3xl                         <-- inner container
    //       main                                       <-- SurfaceHost

    // Check Simple mode: the container has justify-center
    const hasJustifyCenter = await page.evaluate(() => {
      const main = document.querySelector("main");
      if (!main) return false;
      const parent = main.parentElement;
      if (!parent) return false;
      const grandparent = parent.parentElement;
      if (!grandparent) return false;
      return grandparent.classList.contains("justify-center");
    });
    expect(hasJustifyCenter).toBe(true);

    // Switch to Advanced mode
    await page.getByLabel("Advanced mode").click();

    // In Advanced mode, justify-center should be removed
    const hasJustifyCenterAfter = await page.evaluate(() => {
      const main = document.querySelector("main");
      if (!main) return false;
      const parent = main.parentElement;
      if (!parent) return false;
      const grandparent = parent.parentElement;
      if (!grandparent) return false;
      return grandparent.classList.contains("justify-center");
    });
    expect(hasJustifyCenterAfter).toBe(false);
  });
});
