/**
 * Focus management hooks for AETHER Terminal.
 *
 * useFocusTrap — traps Tab focus within a container (for modals, dialogs).
 * useFocusVisible — toggles .focus-visible class on body based on input method.
 */

import { useEffect, useRef } from "react";
import { useStore } from "../state/store";

// ── Focus trap ─────────────────────────────────────────────────────────────

/**
 * Traps Tab focus within a container element.
 *
 * On mount, focuses the first focusable element inside the container.
 * On Tab at the last element, wraps to the first.
 * On Shift+Tab at the first element, wraps to the last.
 * On unmount, restores focus to the previously focused element.
 */
export function useFocusTrap(containerRef: React.RefObject<HTMLElement | null>): void {
  const previousFocusRef = useRef<HTMLElement | null>(null);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    // Save the currently focused element before trapping
    previousFocusRef.current = document.activeElement as HTMLElement;

    // Focus the first focusable element inside the container
    const focusableSelector =
      'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])';
    const firstFocusable = container.querySelector<HTMLElement>(focusableSelector);
    firstFocusable?.focus();

    function handleKeyDown(e: KeyboardEvent) {
      if (e.key !== "Tab") return;

      const focusableElements = container!.querySelectorAll<HTMLElement>(focusableSelector);
      if (focusableElements.length === 0) return;

      const first = focusableElements[0]!;
      const last = focusableElements[focusableElements.length - 1]!;

      if (e.shiftKey) {
        if (document.activeElement === first) {
          e.preventDefault();
          last.focus();
        }
      } else {
        if (document.activeElement === last) {
          e.preventDefault();
          first.focus();
        }
      }
    }

    container.addEventListener("keydown", handleKeyDown);

    return () => {
      container?.removeEventListener("keydown", handleKeyDown);
      // Restore focus to the element that was focused before the trap
      previousFocusRef.current?.focus();
    };
  }, [containerRef]);
}

// ── Focus visible ──────────────────────────────────────────────────────────

/**
 * Toggles `.focus-visible` class on `<body>` based on input method.
 *
 * - Adds class and sets focusMode to "keyboard" on Tab keypress
 * - Removes class and sets focusMode to "mouse" on mouse click
 *
 * CSS in globals.css shows focus rings only when body.focus-visible is set.
 */
export function useFocusVisible(): void {
  const setFocusMode = useStore((s) => s.setFocusMode);

  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === "Tab") {
        document.body.classList.add("focus-visible");
        setFocusMode("keyboard");
      }
    }

    function handleMouseDown() {
      document.body.classList.remove("focus-visible");
      setFocusMode("mouse");
    }

    document.addEventListener("keydown", handleKeyDown);
    document.addEventListener("mousedown", handleMouseDown);

    return () => {
      document.removeEventListener("keydown", handleKeyDown);
      document.removeEventListener("mousedown", handleMouseDown);
    };
  }, [setFocusMode]);
}
