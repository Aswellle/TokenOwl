import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, act } from "@testing-library/react";

const { mockStartDragging, mockOnMoved, mockOpenDashboard, mockPersistPosition } = vi.hoisted(() => ({
  mockStartDragging: vi.fn().mockResolvedValue(undefined),
  // The real `onMoved` resolves directly to the unlisten FUNCTION (unlike
// `listen`, whose promise resolves to `{ unlisten }`); resolving to an
// object makes the effect cleanup call a non-function.
  mockOnMoved: vi.fn(() => Promise.resolve(vi.fn())),
  mockOpenDashboard: vi.fn().mockResolvedValue(undefined),
  mockPersistPosition: vi.fn().mockResolvedValue(undefined),
}));

// The real icon pack ships emoji data as JSON imports that vitest's
// transform rejects; the drag tests do not depend on the icon glyphs.
vi.mock("@lobehub/icons", () => ({ ProviderIcon: () => null }));

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

vi.mock("../lib/tauri", () => ({
  // Drag and move-persistence are desktop-only paths; the tests below exist
  // to exercise exactly those, so the preview pretends to be the real shell.
  isTauri: vi.fn(() => true),
  shell: {
    openDashboard: mockOpenDashboard,
    showBubble: vi.fn().mockResolvedValue(undefined),
    persistBubblePosition: mockPersistPosition,
    openProjectPage: vi.fn(),
  },
  tauriBridge: {
    listAccounts: vi.fn().mockResolvedValue([]),
    createAccount: vi.fn(),
    accountTargetIsLan: vi.fn().mockResolvedValue(false),
    lanConsentBegin: vi.fn().mockResolvedValue(null),
    deleteAccount: vi.fn(),
    refresh: vi.fn(),
    checkForUpdate: vi.fn(),
    installUpdate: vi.fn(),
    listNotifications: vi.fn().mockResolvedValue([]),
    markNotificationsRead: vi.fn(),
    getNotificationPreferences: vi.fn(),
    setNotificationPreferences: vi.fn(),
    historySummary: vi.fn(),
    historyBreakdown: vi.fn().mockResolvedValue([]),
    providerMatrix: vi.fn().mockResolvedValue([]),
    manifestProfiles: vi.fn().mockResolvedValue({ profiles: [], skipped: [] }),
    usageWindows: vi.fn(),
  },
}));

import { BubbleView } from "../features/bubble/BubbleView";

describe("BubbleView drag interactions", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders the bubble button with a readable label", async () => {
    render(<BubbleView />);
    const button = await screen.findByRole("button");
    expect(button).toBeInTheDocument();
    expect(button.getAttribute("aria-label")).toContain("TokenOwl");
  });

  it("opens the dashboard on a click without drag", async () => {
    render(<BubbleView />);
    fireEvent.click(await screen.findByRole("button"));
    expect(mockOpenDashboard).toHaveBeenCalledOnce();
  });

  it("starts native dragging only once the threshold is exceeded", async () => {
    render(<BubbleView />);
    const button = await screen.findByRole("button");

    fireEvent.pointerDown(button, { clientX: 100, clientY: 100, button: 0, isPrimary: true });
    fireEvent.pointerMove(button, { clientX: 103, clientY: 100, isPrimary: true });
    expect(mockStartDragging).not.toHaveBeenCalled();

    fireEvent.pointerMove(button, { clientX: 120, clientY: 100, isPrimary: true });
    expect(mockStartDragging).toHaveBeenCalledOnce();
  });

  it("does not open the dashboard when the press turned into a drag", async () => {
    render(<BubbleView />);
    const button = await screen.findByRole("button");

    fireEvent.pointerDown(button, { clientX: 100, clientY: 100, button: 0, isPrimary: true });
    fireEvent.pointerMove(button, { clientX: 120, clientY: 100, isPrimary: true });
    fireEvent.pointerUp(button);
    fireEvent.click(button);

    expect(mockOpenDashboard).not.toHaveBeenCalled();
  });

  it("ignores non-primary pointers", async () => {
    render(<BubbleView />);
    const button = await screen.findByRole("button");
    fireEvent.pointerDown(button, { clientX: 100, clientY: 100, button: 0, isPrimary: false });
    fireEvent.pointerMove(button, { clientX: 130, clientY: 100, isPrimary: false });
    expect(mockStartDragging).not.toHaveBeenCalled();
  });

  it("persists the position after a native move settles", async () => {
    render(<BubbleView />);

    // The desktop-only effect subscribes on mount; emit one window move and
    // let the debounced persist fire.
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
});
