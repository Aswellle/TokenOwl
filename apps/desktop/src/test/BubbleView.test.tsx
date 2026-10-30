import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, act, waitFor } from "@testing-library/react";

const {
  mockStartDragging,
  mockOnMoved,
  mockOpenDashboard,
  mockPersistPosition,
  mockListAccounts,
  mockStabilize,
} = vi.hoisted(() => ({
  mockStartDragging: vi.fn().mockResolvedValue(undefined),
  // The real `onMoved` resolves directly to the unlisten FUNCTION (unlike
  // `listen`, whose promise resolves to `{ unlisten }`); resolving to an
  // object makes the effect cleanup call a non-function.
  mockOnMoved: vi.fn((_callback: () => void) => Promise.resolve(vi.fn())),
  mockOpenDashboard: vi.fn().mockResolvedValue(undefined),
  mockPersistPosition: vi.fn().mockResolvedValue(undefined),
  mockListAccounts: vi.fn(),
  mockStabilize: vi.fn().mockResolvedValue(undefined),
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
    stabilizeBubbleWindow: mockStabilize,
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
import { gsap } from "../lib/animation/gsap";
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
    const values = document.querySelectorAll(".quota-rings__value");
    expect(values).toHaveLength(2);
    expect(values[0].getAttribute("r")).toBe("21");
    expect(values[1].getAttribute("r")).toBe("14");
    expect(document.querySelectorAll(".quota-rings__track")).toHaveLength(2);
    // No shared channel circle: the gap between the bands stays clean.
    expect(document.querySelector(".quota-rings__channel")).toBeNull();
    // Tones carry the ring position, not the provider.
    expect(values[0].getAttribute("class")).toContain("quota-rings__value--outer");
    expect(values[1].getAttribute("class")).toContain("quota-rings__value--inner");
  });

  it("keeps the dial unlit instead of drawing a stray dot at zero or unknown", async () => {
    mockListAccounts.mockResolvedValue(
      // Remaining 100 of 100 → 0% consumed; a null ratio is unknown.
      accountWith(windowedQuota("q-empty", 5, 100), windowedQuota("q-unknown", 168, null)),
    );
    render(<BubbleView />);
    await screen.findByRole("button");

    // Both bands still exist as tracks, but no value arc is drawn: a round
    // line cap at zero would leave a dot on an empty dial, and unknown must
    // never be painted as empty progress.
    expect(document.querySelectorAll(".quota-rings__value")).toHaveLength(0);
    expect(document.querySelectorAll(".quota-rings__track")).toHaveLength(2);
    expect(
      document.querySelector('[data-ring-id="q-empty"]')?.getAttribute("data-ring-consumed"),
    ).toBe("0.000");
    expect(
      document.querySelector('[data-ring-id="q-unknown"]')?.getAttribute("data-ring-consumed"),
    ).toBe("unknown");
  });

  it("fills the dial and butts the seam when the quota is fully consumed", async () => {
    mockListAccounts.mockResolvedValue(accountWith(windowedQuota("q-full", 5, 0)));
    render(<BubbleView />);
    await screen.findByRole("button");

    const value = document.querySelector(".quota-rings__value");
    expect(value).not.toBeNull();
    // GSAP owns the offset; at 100% consumed the dial is fully drawn.
    await waitFor(() =>
      expect(Number(gsap.getProperty(value as Element, "strokeDashoffset"))).toBeCloseTo(0, 1),
    );
    // Two round caps meeting at 12 o'clock would bulge the seam.
    expect(value?.getAttribute("stroke-linecap")).toBe("butt");
  });

  it("keeps drawing the last known rings while the account is stale", async () => {
    mockListAccounts.mockResolvedValue([
      { ...accountWith(windowedQuota("q-5h", 5, 40))[0], status: "stale" },
    ]);
    render(<BubbleView />);
    const button = await screen.findByRole("button");
    // Stale data is still the last known truth: the ring keeps its value
    // instead of collapsing to zero.
    expect(document.querySelectorAll(".quota-rings__value")).toHaveLength(1);
    expect(document.querySelector('[data-ring-tone="outer"]')?.getAttribute("data-ring-consumed")).toBe(
      "0.600",
    );
    expect(button.getAttribute("aria-label")).toContain("5h 60%");
  });

  it("shows only the owl when the local service is offline and nothing is cached", async () => {
    mockListAccounts.mockRejectedValue(new Error("ipc down"));
    render(<BubbleView />);
    await screen.findByRole("button");
    expect(document.querySelector(".bubble__mark--solo")).not.toBeNull();
    expect(document.querySelectorAll(".quota-rings__value")).toHaveLength(0);
    expect(document.querySelectorAll(".quota-rings__track")).toHaveLength(0);
  });

  it("reports consumed percentages in the accessible name", async () => {
    mockListAccounts.mockResolvedValue(accountWith(windowedQuota("q-5h", 5, 40)));
    render(<BubbleView />);
    const button = await screen.findByRole("button");
    expect(button.getAttribute("aria-label")).toContain("额度已用：5h 60%");
    expect(button.getAttribute("aria-label")).not.toContain("剩余");
  });

  it("renders only the owl when no provider is configured", async () => {
    mockListAccounts.mockResolvedValue([]);
    render(<BubbleView />);
    await screen.findByRole("button");

    expect(document.querySelector(".bubble__mark--solo")).not.toBeNull();
    expect(document.querySelectorAll(".quota-rings__track")).toHaveLength(0);
    expect(document.querySelectorAll(".quota-rings__value")).toHaveLength(0);
  });

  it("re-applies the tool-window shape once the first load settles", async () => {
    mockListAccounts.mockResolvedValue([]);
    render(<BubbleView />);
    await screen.findByRole("button");

    // The shell rewrites window flags on every show, so the webview pins the
    // tool-window style and 56×56 size back once its data has settled.
    expect(mockStabilize).toHaveBeenCalled();
  });
});
