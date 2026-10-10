import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { StatusChip } from "../features/dashboard/components/StatusChip";

describe("StatusChip", () => {
  it("renders healthy status with text", () => {
    const { container } = render(<StatusChip status="healthy" />);
    expect(container.querySelector(".chip")).toBeInTheDocument();
    expect(container.textContent).toBeTruthy();
  });

  it("renders error status", () => {
    const { container } = render(<StatusChip status="error" />);
    expect(container.querySelector(".chip")).toBeInTheDocument();
  });

  it("renders syncing status", () => {
    const { container } = render(<StatusChip status="syncing" />);
    expect(container.querySelector(".chip")).toBeInTheDocument();
  });

  it("renders stale status", () => {
    const { container } = render(<StatusChip status="stale" />);
    expect(container.querySelector(".chip")).toBeInTheDocument();
  });

  it("renders offline status", () => {
    const { container } = render(<StatusChip status="offline" />);
    expect(container.querySelector(".chip")).toBeInTheDocument();
  });

  it("applies correct CSS class per status", () => {
    const { container: healthyContainer } = render(<StatusChip status="healthy" />);
    expect(healthyContainer.querySelector(".chip--healthy")).toBeInTheDocument();

    const { container: errorContainer } = render(<StatusChip status="error" />);
    expect(errorContainer.querySelector(".chip--error")).toBeInTheDocument();
  });
});
