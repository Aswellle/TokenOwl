import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, act } from "@testing-library/react";

const { mockStartDragging, mockOnMoved, mockOpenDashboard, mockPersistPosition } = vi.hoisted(() => ({
  mockStartDragging: vi.fn().mockResolvedValue(undefined),
  mockOnMoved: vi.fn().mockResolvedValue({ unlisten: vi.fn().mockResolvedValue(undefined) }),
  mockOpenDashboard: vi.fn().mockResolvedValue(undefined),
  mockPersistPosition: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    startDragging: mockStartDragging,
    onMoved: mockOnMoved,
    outerPosition: vi.fn().mockResolvedValue({ x: 100, y: 200 }),
  }),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: () => new Promise(() => {}),
}));

vi.mock("../../lib/tauri", () => ({
  shell: {
    openDashboard: mockOpenDashboard,
    showBubble: vi.fn().mockResolvedValue(undefined),
    persistBubblePosition: mockPersistPosition,
  },
  tauriBridge: {
    listAccounts: vi.fn().mockResolvedValue([]),
    createAccount: vi.fn(),
    deleteAccount: vi.fn(),
    refresh: vi.fn(),
    checkForUpdate: vi.fn(),
    installUpdate: vi.fn(),
  },
}));

import { BubbleView } from "../features/bubble/BubbleView";

describe("BubbleView drag interactions", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders bubble button", () => {
    render(<BubbleView />);
    const button = screen.getByRole("button");
    expect(button).toBeInTheDocument();
  });

  it("opens dashboard on click without drag", () => {
    render(<BubbleView />);
    const button = screen.getByRole("button");
    fireEvent.click(button);
    expect(mockOpenDashboard).toHaveBeenCalledOnce();
  });

  it("does not open dashboard when drag threshold exceeded", () => {
    render(<BubbleView />);
    const button = screen.getByRole("button");

    fireEvent.pointerDown(button, { clientX: 100, clientY: 100, button: 0, isPrimary: true });
    fireEvent.pointerMove(button, { clientX: 120, clientY: 100, isPrimary: true });

    expect(mockStartDragging).toHaveBeenCalledOnce();

    fireEvent.pointerUp(button);
    fireEvent.click(button);

    expect(mockOpenDashboard).not.toHaveBeenCalled();
  });

  it("does not start dragging below threshold", () => {
    render(<BubbleView />);
    const button = screen.getByRole("button");
    fireEvent.pointerDown(button, { clientX: 100, clientY: 100, button: 0, isPrimary: true });
    fireEvent.pointerMove(button, { clientX: 103, clientY: 100, isPrimary: true });
    expect(mockStartDragging).not.toHaveBeenCalled();
  });

  it("ignores non-primary pointers", () => {
    render(<BubbleView />);
    const button = screen.getByRole("button");
    fireEvent.pointerDown(button, { clientX: 100, clientY: 100, button: 0, isPrimary: false });
    expect(mockStartDragging).not.toHaveBeenCalled();
  });

  it("persists position after drag settles", async () => {
    render(<BubbleView />);

    await act(async () => {
      mockOnMoved.mock.calls[0][0]();
    });

    await act(async () => {
      const { promise, resolve } = Promise.withResolvers<void>();
      setTimeout(resolve, 500);
      await promise;
    });

    expect(mockPersistPosition).toHaveBeenCalled();
  });

  it("shows correct aria-label", () => {
    render(<BubbleView />);
    const button = screen.getByRole("button");
    expect(button).toHaveAttribute("aria-label");
    expect(button.getAttribute("aria-label")).toContain("TokenOwl");
  });
});
