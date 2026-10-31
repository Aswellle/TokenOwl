import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
// jsdom/Node cannot parse recharts' transitive JSON import chain; the chart
// itself is covered by the browser audit, so stub it here.
// @lobehub/icons pulls a transitive JSON import that Node ESM in vitest
// rejects; brand icons render via the browser audit, so stub them here.
vi.mock("../../components/ProviderIcon", () => ({
  ProviderIcon: () => null,
}));
vi.mock("@lobehub/icons", () => ({
  ProviderIcon: () => null,
}));
vi.mock("recharts", () => ({
  AreaChart: ({ children }: { children?: React.ReactNode }) => <div>{children}</div>,
  Area: () => null,
  ResponsiveContainer: ({ children }: { children?: React.ReactNode }) => <div>{children}</div>,
  XAxis: () => null,
  YAxis: () => null,
  Tooltip: () => null,
}));
import { HistorySection } from "../features/dashboard/components/HistorySection";
import type {
  DashboardBridge,
  HistorySummaryViewModel,
} from "../types";

const todaySeconds = Math.floor(new Date(new Date().toDateString()).getTime() / 1000);
const day = 86_400;

const summaryWithData: HistorySummaryViewModel = {
  days: [
    {
      dayStart: todaySeconds - 2 * day,
      tokens: 0,
      cost: null,
      currency: null,
      hasData: true,
    },
    {
      dayStart: todaySeconds - day,
      tokens: 500_000,
      cost: 2.25,
      currency: "¥",
      hasData: true,
    },
    {
      dayStart: todaySeconds,
      tokens: 1_200_000,
      cost: 5.4,
      currency: "¥",
      hasData: true,
    },
  ],
  totalTokens: 1_700_000,
  totalCost: 7.65,
  currency: "¥",
  activeDays: 2,
  rangeDays: 182,
};

const summaryEmpty: HistorySummaryViewModel = {
  days: [{ dayStart: todaySeconds, tokens: 0, cost: null, currency: null, hasData: false }],
  totalTokens: 0,
  totalCost: null,
  currency: null,
  activeDays: 0,
  rangeDays: 182,
};

function makeBridge(summary: HistorySummaryViewModel): DashboardBridge {
  return {
    listAccounts: vi.fn(async () => []),
    accountTargetIsLan: vi.fn(async () => false),
    lanConsentBegin: vi.fn().mockResolvedValue(null),
    createAccount: vi.fn(async () => {
      throw new Error("not used");
    }),
    deleteAccount: vi.fn(async () => {}),
    refresh: vi.fn(async () => []),
    checkForUpdate: vi.fn(async () => null),
    installUpdate: vi.fn(async () => {}),
    listNotifications: vi.fn(async () => []),
    markNotificationsRead: vi.fn(async () => {}),
    getNotificationPreferences: vi.fn(async () => {
      throw new Error("not used");
    }),
    setNotificationPreferences: vi.fn(async () => {}),
    historySummary: vi.fn(async () => structuredClone(summary)),
    historyBreakdown: vi.fn(async () => []),
    providerMatrix: vi.fn(async () => []),
    manifestProfiles: vi.fn(async () => ({ profiles: [], skipped: [] })),
    usageWindows: vi.fn(async () => ({
      hours2: { tokens: 0, cost: null, currency: null, hasData: false },
      hours24: { tokens: 0, cost: null, currency: null, hasData: false },
      days30: { tokens: 0, cost: null, currency: null, hasData: false },
    })),
  };
}

describe("HistorySection", () => {
  it("renders the 30-day summary stats with compact numbers", async () => {
    render(<HistorySection bridge={makeBridge(summaryWithData)} accounts={[]} reloadSignal={0} />);
    expect(await screen.findByText("近 30 天 Tokens")).toBeInTheDocument();
    expect(screen.getByText("170 万")).toBeInTheDocument();
    expect(screen.getByText("¥7.65")).toBeInTheDocument();
    expect(
      screen.getByText(
        (_, element) => element?.classList.contains("num") === true && element.textContent === "2/30",
      ),
    ).toBeInTheDocument();
  });

  it("renders day cells with accessible date and usage labels", async () => {
    render(<HistorySection bridge={makeBridge(summaryWithData)} accounts={[]} reloadSignal={0} />);
    const labels = await screen.findAllByText(/Tokens/);
    expect(labels.length).toBeGreaterThanOrEqual(3);
    expect(screen.getByText("少")).toBeInTheDocument();
    expect(screen.getByText("多")).toBeInTheDocument();
  });

  it("shows the empty state when nothing has been recorded", async () => {
    render(<HistorySection bridge={makeBridge(summaryEmpty)} accounts={[]} reloadSignal={0} />);
    expect(await screen.findByText("历史从每次自动刷新开始逐日累积。添加一个账户并等待下一次刷新，这里就会出现第一天的用量。")).toBeInTheDocument();
  });

  it("moves cell focus with arrow keys inside the heatmap", async () => {
    render(<HistorySection bridge={makeBridge(summaryWithData)} accounts={[]} reloadSignal={0} />);
    const heatmap = await screen.findByRole("group", { name: "每日用量热力图" });
    // Today is always the last column, so ArrowLeft always has a target.
    fireEvent.keyDown(heatmap, { key: "ArrowLeft" });
    expect(document.activeElement?.className).toContain("heat-cell");
    const first = document.activeElement;
    if (!(first instanceof HTMLElement)) throw new Error("focus did not move into the heatmap");
    fireEvent.keyDown(first, { key: "ArrowLeft" });
    expect(document.activeElement).not.toBe(first);
    expect(document.activeElement?.className).toContain("heat-cell");
  });

  it("reports a failed read with a retry instead of an endless skeleton", async () => {
    const bridge = makeBridge(summaryWithData);
    bridge.historySummary = vi.fn(async () => {
      throw new Error("offline");
    });
    render(<HistorySection bridge={bridge} accounts={[]} reloadSignal={0} />);
    expect(
      await screen.findByText("历史数据暂时无法读取，请稍后重试。"),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "重试" })).toBeInTheDocument();
    expect(document.querySelector(".history-section .skeleton")).toBeNull();
  });
});
