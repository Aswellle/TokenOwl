import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen } from "@testing-library/react";
import { ErrorBoundary } from "../components/ErrorBoundary";

function Boom(): never {
  throw new Error("render exploded");
}

describe("ErrorBoundary", () => {
  beforeEach(() => {
    // React logs the caught error; keep the test output clean but assert it ran.
    vi.spyOn(console, "error").mockImplementation(() => {});
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("renders children while nothing throws", () => {
    render(
      <ErrorBoundary surface="仪表盘">
        <p>healthy content</p>
      </ErrorBoundary>,
    );
    expect(screen.getByText("healthy content")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("replaces a crashed tree with a recoverable message instead of a blank window", () => {
    render(
      <ErrorBoundary surface="仪表盘">
        <Boom />
      </ErrorBoundary>,
    );
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("仪表盘");
    expect(screen.getByRole("button", { name: "重新加载" })).toBeInTheDocument();
  });

  it("never echoes the thrown message, which may carry provider payload text", () => {
    render(
      <ErrorBoundary surface="悬浮球">
        <Boom />
      </ErrorBoundary>,
    );
    expect(screen.queryByText(/render exploded/)).not.toBeInTheDocument();
  });
});
