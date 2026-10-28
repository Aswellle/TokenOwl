import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, act } from "@testing-library/react";

const { mockStartDragging, mockOnMoved, mockOpenDashboard, mockPersistPosition, mockListAccounts } =
  vi.hoisted(() => ({
    mockStartDragging: vi.fn().mockResolvedValue(undefined),
    // The real `onMoved` resolves directly to the unlisten FUNCTION (unlike
    // `listen`, whose promise resolves to `{ unlisten }`); resolving to an
    // object makes the effect cleanup call a non-function.
    mockOnMoved: vi.fn((_callback: () => void) => Promise.resolve(vi.fn())),
    mockOpenDashboard: vi.fn().mockResolvedValue(undefined),
    mockPersistPosition: vi.fn().mockResolvedValue(undefined),
    mockListAccounts: vi.fn(),
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
    listAccounts: mockListAccounts,
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
import type { AccountViewModel, QuotaViewModel } from "../types";

describe("BubbleView drag interactions", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockListAccounts.mockResolvedValue([]);
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
    const handler = mockOnMoved.mock.calls.at(0)?.[0];
    expect(handler).toBeTypeOf("function");
    await act(async () => {
      handler?.();
    });
    await act(async () => {
      const { promise, resolve } = Promise.withResolvers<void>();
      setTimeout(resolve, 500);
      await promise;
    });

    expect(mockPersistPosition).toHaveBeenCalled();
  });
});

/** A windowed quota whose span reads as `${hours}h`/`${days}d` in the tag. */
function windowedQuota(id: string, spanHours: number, remaining: number | null): QuotaViewModel {
  const now = Date.now();
  return {
    id,
    metricName: `${spanHours}h 窗口`,
    unit: "token",
    used: remaining === null ? null : 100 - remaining,
    limit: remaining === null ? null : 100,
    remaining,
    window: {
      type: "rolling",
      startsAt: new Date(now - spanHours * 3_600_000).toISOString(),
      endsAt: new Date(now + 60_000).toISOString(),
    },
    observedAt: now,
    source: "OFFICIAL_API",
    freshness: "fresh",
    status: "available",
  };
}

function accountWith(...snapshots: QuotaViewModel[]): AccountViewModel[] {
  return [
    {
      id: "acc-1",
      providerId: "openai:acc-1",
      providerName: "OpenAI",
      label: "Test Account",
      status: "healthy",
      freshness: "fresh",
      snapshots,
    },
  ];
}

describe("bubble ring geometry", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("draws two separated rings for a provider with two windowed quotas", async () => {
    mockListAccounts.mockResolvedValue(
      accountWith(windowedQuota("q-5h", 5, 60), windowedQuota("q-7d", 168, 80)),
    );
    render(<BubbleView />);
    await screen.findByRole("button");

    // The shortest window runs on the outer radius (21), the longer one on
    // the inner radius (14); each band carries its own unlit track.
    const values = document.querySelectorAll(".bubble__ring-value");
    expect(values).toHaveLength(2);
    expect(values[0].getAttribute("r")).toBe("21");
    expect(values[1].getAttribute("r")).toBe("14");
    expect(document.querySelectorAll(".bubble__ring-track")).toHaveLength(2);
    // No shared channel circle: the gap between the bands stays clean.
    expect(document.querySelector(".bubble__ring-channel")).toBeNull();
  });

  it("keeps the dial unlit instead of drawing a stray dot at zero or unknown", async () => {
    mockListAccounts.mockResolvedValue(
      accountWith(windowedQuota("q-empty", 5, 0), windowedQuota("q-unknown", 168, null)),
    );
    render(<BubbleView />);
    await screen.findByRole("button");

    // Both bands still exist as tracks, but no value arc is drawn: a round
    // line cap at zero would leave a dot on an empty dial.
    expect(document.querySelectorAll(".bubble__ring-value")).toHaveLength(0);
    expect(document.querySelectorAll(".bubble__ring-track")).toHaveLength(2);
  });

  it("renders only the owl when no provider is configured", async () => {
    mockListAccounts.mockResolvedValue([]);
    render(<BubbleView />);
    await screen.findByRole("button");

    expect(document.querySelector(".bubble__mark--solo")).not.toBeNull();
    expect(document.querySelectorAll(".bubble__ring-track")).toHaveLength(0);
    expect(document.querySelectorAll(".bubble__ring-value")).toHaveLength(0);
  });
});
