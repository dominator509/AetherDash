import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./app";
import { useStore } from "./state/store";
import "./styles/globals.css";

// Expose the zustand store on window in dev mode so Playwright e2e
// tests can mutate state directly (e.g., setting connectionStatus
// when no real gateway is available).
if (import.meta.env.DEV) {
  (window as unknown as Record<string, unknown>).__AETHER_STORE__ = useStore;
}

const rootElement = document.getElementById("root");
if (!rootElement) {
  throw new Error("Root element #root not found in the DOM");
}

createRoot(rootElement).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
