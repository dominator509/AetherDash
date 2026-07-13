/**
 * Tests for state components: LoadingSkeleton, EmptyState, ErrorState, DegradationBanner.
 */

import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { LoadingSkeleton } from "../states/LoadingSkeleton";
import { EmptyState } from "../states/EmptyState";
import { ErrorState } from "../states/ErrorState";
import { DegradationBanner } from "../states/DegradationBanner";

describe("LoadingSkeleton", () => {
  it("renders without crashing", () => {
    const { container } = render(<LoadingSkeleton />);
    expect(container.querySelector('[role="status"]')).toBeTruthy();
  });

  it("renders default number of rows (3)", () => {
    const { container } = render(<LoadingSkeleton />);
    const statusEl = container.querySelector('[role="status"]');
    const rows = statusEl?.querySelectorAll(":scope > div");
    expect(rows?.length).toBe(3);
  });

  it("renders custom number of rows", () => {
    const { container } = render(<LoadingSkeleton rows={5} />);
    const statusEl = container.querySelector('[role="status"]');
    const rows = statusEl?.querySelectorAll(":scope > div");
    expect(rows?.length).toBe(5);
  });

  it("renders custom columns per row", () => {
    const { container } = render(<LoadingSkeleton rows={2} columns={3} />);
    const statusEl = container.querySelector('[role="status"]');
    const firstRow = statusEl?.querySelector(":scope > div");
    expect(firstRow?.childNodes.length).toBe(3);
  });
});

describe("EmptyState", () => {
  it("renders message", () => {
    render(<EmptyState message="No data available" />);
    expect(screen.getByText("No data available")).toBeTruthy();
  });

  it("renders action button with label when onAction provided", () => {
    const onAction = vi.fn();
    render(<EmptyState message="Nothing here" actionLabel="Connect" onAction={onAction} />);
    const btn = screen.getByRole("button", { name: "Connect" });
    expect(btn).toBeTruthy();
    btn.click();
    expect(onAction).toHaveBeenCalledTimes(1);
  });

  it("does not render action button when onAction is missing", () => {
    render(<EmptyState message="Nothing here" actionLabel="Connect" />);
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("does not render action button when actionLabel is missing", () => {
    render(<EmptyState message="Nothing here" onAction={vi.fn()} />);
    expect(screen.queryByRole("button")).toBeNull();
  });
});

describe("ErrorState", () => {
  it("renders error code", () => {
    render(
      <ErrorState
        code="unavailable"
        message="Service is down"
        trace_id="01ABCDEFGHIJKLMNOPQRSTUVWX"
      />,
    );
    expect(screen.getByText("unavailable")).toBeTruthy();
  });

  it("renders error message", () => {
    render(
      <ErrorState
        code="unavailable"
        message="Service is down"
        trace_id="01ABCDEFGHIJKLMNOPQRSTUVWX"
      />,
    );
    expect(screen.getByText("Service is down")).toBeTruthy();
  });

  it("renders trace_id", () => {
    render(<ErrorState code="unavailable" message="Service is down" trace_id="TRACE123" />);
    expect(screen.getByText("TRACE123")).toBeTruthy();
  });

  it("shows retry button when retryable and onRetry provided", () => {
    const onRetry = vi.fn();
    render(
      <ErrorState
        code="unavailable"
        message="Service is down"
        trace_id="TRACE123"
        retryable={true}
        onRetry={onRetry}
      />,
    );
    const btn = screen.getByRole("button", { name: "Retry" });
    expect(btn).toBeTruthy();
    btn.click();
    expect(onRetry).toHaveBeenCalledTimes(1);
  });

  it("does NOT show retry button when retryable is false", () => {
    render(
      <ErrorState
        code="invalid_argument"
        message="Bad input"
        trace_id="TRACE123"
        retryable={false}
      />,
    );
    expect(screen.queryByRole("button", { name: "Retry" })).toBeNull();
  });

  it("does NOT show retry button when onRetry is missing even if retryable", () => {
    render(
      <ErrorState
        code="unavailable"
        message="Service is down"
        trace_id="TRACE123"
        retryable={true}
      />,
    );
    expect(screen.queryByRole("button", { name: "Retry" })).toBeNull();
  });
});

describe("DegradationBanner", () => {
  it("renders surface name", () => {
    render(
      <DegradationBanner
        surface="feed"
        reason="Upstream venue latency spike"
        started_at="2026-07-11T12:00:00.000Z"
      />,
    );
    expect(screen.getByText("feed")).toBeTruthy();
  });

  it("renders reason", () => {
    render(
      <DegradationBanner
        surface="explain"
        reason="LLM inference degraded"
        started_at="2026-07-11T12:00:00.000Z"
      />,
    );
    expect(screen.getByText("LLM inference degraded")).toBeTruthy();
  });

  it("has role status with aria-live polite", () => {
    render(
      <DegradationBanner surface="feed" reason="Test" started_at="2026-07-11T12:00:00.000Z" />,
    );
    const el = screen.getByRole("status");
    expect(el).toBeTruthy();
    expect(el.getAttribute("aria-live")).toBe("polite");
  });
});
