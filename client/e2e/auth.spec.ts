/**
 * Authentication E2E tests for AETHER Terminal.
 *
 * Verifies the login screen appears when no session token exists,
 * token validation flow, error handling, keyboard accessibility, and
 * session restore on page reload.
 */

import { test, expect } from "@playwright/test";

/** Gateway auth endpoint URL (matches DEFAULT_GATEWAY_URL + /auth/validate). */
const AUTH_URL = "http://localhost:8080/auth/validate";

test.describe("Authentication", () => {
  test("Login screen appears when no session token exists", async ({ page }) => {
    await page.goto("/");

    // Login screen branding should be visible
    await expect(page.getByText("AETHER Terminal")).toBeVisible();
    await expect(page.getByText("Authenticate to connect")).toBeVisible();

    // Login form field should be present
    await expect(page.getByLabel("Session Token")).toBeVisible();
    await expect(page.getByRole("button", { name: "Connect" })).toBeVisible();

    // Shell should NOT be visible
    await expect(page.getByLabel("Feed")).not.toBeVisible();
  });

  test("Entering a valid token and clicking Connect transitions to shell", async ({ page }) => {
    // Mock the auth validate endpoint to return a valid response
    await page.route(AUTH_URL, async (route) => {
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({ valid: true, actor_id: "test-user", tier: 3 }),
      });
    });

    await page.goto("/");

    // Fill in session token
    await page.getByLabel("Session Token").fill("my-session-token-12345");

    // Click Connect
    await page.getByRole("button", { name: "Connect" }).click();

    // After successful auth, the shell should appear (NavRail visible)
    await expect(page.getByLabel("Feed")).toBeVisible({ timeout: 5000 });
  });

  test("Invalid token shows error message", async ({ page }) => {
    // Mock the auth validate endpoint to return 401
    await page.route(AUTH_URL, async (route) => {
      await route.fulfill({
        status: 401,
        statusText: "Unauthorized",
      });
    });

    await page.goto("/");

    // Fill in an invalid token
    await page.getByLabel("Session Token").fill("bad-token");

    // Click Connect
    await page.getByRole("button", { name: "Connect" }).click();

    // Error message should appear
    const errorMessage = page.getByRole("alert");
    await expect(errorMessage).toBeVisible({ timeout: 5000 });
    await expect(errorMessage).toContainText("Invalid token");
  });

  test("Keyboard-only login (Tab + Enter)", async ({ page }) => {
    // Mock the auth validate endpoint to return a valid response
    await page.route(AUTH_URL, async (route) => {
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({ valid: true, actor_id: "keyboard-user", tier: 3 }),
      });
    });

    await page.goto("/");

    // Tab to token field and type
    await page.keyboard.press("Tab");
    await page.keyboard.type("my-session-token-67890");

    // Press Enter while the token field is focused.
    // In any browser, pressing Enter in a text input inside a <form> with a
    // <button type="submit"> triggers form submission.
    await page.keyboard.press("Enter");

    // After successful auth, shell should appear
    await expect(page.getByLabel("Feed")).toBeVisible({ timeout: 5000 });
  });

  test("Session restores on page reload with stored token (keychain mock)", async ({ page }) => {
    // Inject mocks for Tauri IPC so that @tauri-apps/api/core's invoke()
    // uses our mock instead of calling the actual Tauri backend.
    //
    // The @tauri-apps/api/core invoke() function calls:
    //   window.__TAURI_INTERNALS__.invoke(cmd, args, options)
    //
    // We set that up here along with the callback infrastructure that the
    // Tauri IPC layer expects.
    // NOTE: addInitScript serializes the function as a string, so plain JS only.
    await page.addInitScript(() => {
      window.__TAURI_INTERNALS__ = window.__TAURI_INTERNALS__ || {};
      window.__TAURI_EVENT_PLUGIN_INTERNALS__ = window.__TAURI_EVENT_PLUGIN_INTERNALS__ || {};

      // Mock IPC: return a stored session token from the OS keychain plugin
      window.__TAURI_INTERNALS__.invoke = async function (cmd) {
        if (cmd === "get_session_token") {
          return "persistent-keychain-token-abc";
        }
        if (cmd === "set_session_token") {
          return;
        }
        if (cmd === "delete_session_token") {
          return;
        }
        return null;
      };

      // Callback infrastructure required by the Tauri IPC layer
      var callbacks = new Map();
      window.__TAURI_INTERNALS__.transformCallback = function (callback, once) {
        once = once === true;
        var identifier = window.crypto.getRandomValues(new Uint32Array(1))[0];
        callbacks.set(identifier, function (data) {
          if (once) callbacks.delete(identifier);
          if (typeof callback === "function") return callback(data);
        });
        return identifier;
      };
      window.__TAURI_INTERNALS__.unregisterCallback = function (id) {
        callbacks.delete(id);
      };
      window.__TAURI_INTERNALS__.runCallback = function (id, data) {
        var cb = callbacks.get(id);
        if (cb) cb(data);
      };
      window.__TAURI_INTERNALS__.callbacks = callbacks;

      // File path conversion mock
      window.__TAURI_INTERNALS__.convertFileSrc = function (filePath) {
        return filePath;
      };

      // Window metadata mock
      window.__TAURI_INTERNALS__.metadata = {
        currentWindow: { label: "main" },
        currentWebview: { windowLabel: "main", label: "main" },
      };
    });

    // Mock the /auth/validate POST to return valid for the stored token.
    // This is called by bootstrap -> validateToken() on page load.
    await page.route(AUTH_URL, async (route) => {
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({ valid: true, actor_id: "test-user", tier: 3 }),
      });
    });

    // Mock the WebSocket connection.
    // After validation succeeds, bootstrap connects the WS with the token.
    // We abort the connection (no real gateway running), but that's fine:
    // setAuthenticated(true) is called synchronously before the WS connects.
    await page.route("**/ws?*", async (route) => {
      await route.abort();
    });

    await page.goto("/");

    // The shell should appear (NavRail visible) WITHOUT entering credentials,
    // proving: keychain token retrieval -> token validation -> authentication -> shell display
    await expect(page.getByLabel("Feed")).toBeVisible({ timeout: 5000 });
  });
});
