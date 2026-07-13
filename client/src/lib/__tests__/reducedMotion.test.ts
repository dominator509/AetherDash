/**
 * Tests for reduced motion utilities.
 *
 * jsdom doesn't implement matchMedia, so we mock it.
 * These tests verify that:
 * - useReducedMotion() returns false when the media query doesn't match
 * - The body class is toggled correctly
 * - Imperative helpers work
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useReducedMotion, prefersReducedMotion, applyReducedMotionClass } from "../reducedMotion";

// ── matchMedia mock factory ─────────────────────────────────────────────────

function createMatchMediaMock(matches: boolean) {
  const listeners = new Set<EventListener>();
  return vi.fn((query: string) => ({
    matches,
    media: query,
    onchange: null,
    addListener: vi.fn(), // deprecated but some libs use it
    removeListener: vi.fn(),
    addEventListener: (event: string, listener: EventListener) => {
      if (event === "change") listeners.add(listener);
    },
    removeEventListener: (event: string, listener: EventListener) => {
      if (event === "change") listeners.delete(listener);
    },
    dispatchEvent: (event: Event) => {
      listeners.forEach((l) => l(event));
      return true;
    },
  }));
}

// ── Tests ───────────────────────────────────────────────────────────────────

describe("prefersReducedMotion", () => {
  beforeEach(() => {
    document.body.classList.remove("reduced-motion");
  });

  it("returns false when the media query does not match", () => {
    window.matchMedia = createMatchMediaMock(false);
    expect(prefersReducedMotion()).toBe(false);
  });

  it("returns true when prefers-reduced-motion: reduce is set", () => {
    window.matchMedia = createMatchMediaMock(true);
    expect(prefersReducedMotion()).toBe(true);
  });
});

describe("applyReducedMotionClass", () => {
  beforeEach(() => {
    document.body.classList.remove("reduced-motion");
  });

  it("does not add class when not reduced", () => {
    window.matchMedia = createMatchMediaMock(false);
    applyReducedMotionClass();
    expect(document.body.classList.contains("reduced-motion")).toBe(false);
  });

  it("adds reduced-motion class to body when preference is set", () => {
    window.matchMedia = createMatchMediaMock(true);
    applyReducedMotionClass();
    expect(document.body.classList.contains("reduced-motion")).toBe(true);
  });
});

describe("useReducedMotion", () => {
  beforeEach(() => {
    document.body.classList.remove("reduced-motion");
  });

  it("returns false when media query does not match", () => {
    window.matchMedia = createMatchMediaMock(false);
    const { result } = renderHook(() => useReducedMotion());
    expect(result.current).toBe(false);
  });

  it("returns true when prefers-reduced-motion: reduce is set", () => {
    window.matchMedia = createMatchMediaMock(true);
    const { result } = renderHook(() => useReducedMotion());
    expect(result.current).toBe(true);
  });

  it("adds reduced-motion class on mount when preference is set", () => {
    window.matchMedia = createMatchMediaMock(true);
    renderHook(() => useReducedMotion());
    expect(document.body.classList.contains("reduced-motion")).toBe(true);
  });

  it("does not add reduced-motion class when not reduced", () => {
    window.matchMedia = createMatchMediaMock(false);
    renderHook(() => useReducedMotion());
    expect(document.body.classList.contains("reduced-motion")).toBe(false);
  });

  it("responds to media query changes", () => {
    let matches = false;
    const listeners = new Set<EventListener>();

    window.matchMedia = vi.fn(() => ({
      matches,
      media: "(prefers-reduced-motion: reduce)",
      onchange: null,
      addListener: vi.fn(),
      removeListener: vi.fn(),
      addEventListener: (_event: string, listener: EventListener) => {
        listeners.add(listener);
      },
      removeEventListener: (_event: string, listener: EventListener) => {
        listeners.delete(listener);
      },
      dispatchEvent: (_event: Event) => true,
    }));

    const { result } = renderHook(() => useReducedMotion());

    expect(result.current).toBe(false);

    // Simulate change
    act(() => {
      matches = true;
      listeners.forEach((l) => l(new MediaQueryEvent("change")));
    });

    expect(result.current).toBe(true);
  });
});

// ── Helpers ─────────────────────────────────────────────────────────────────

class MediaQueryEvent extends Event {
  matches: boolean;

  constructor(type: string) {
    super(type);
    this.matches = true;
  }
}
