import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { NotificationSection } from "../features/dashboard/components/NotificationSection";
import type {
  AccountViewModel,
  DashboardBridge,
  GenericOpenAiAccountInput,
  NotificationPreferencesViewModel,
  NotificationViewModel,
} from "../types";

const prefs: NotificationPreferencesViewModel = {
  enabled: true,
  silentMode: false,
  criticalOnly: false,
  lowQuotaThreshold: 20,
  balanceThreshold: null,
  resetSoonWindow: 1_800,
};

const items: NotificationViewModel[] = [
  {
    id: "n1",
    kind: "low_quota",
    severity: "CRITICAL",
    title: "额度紧张",
    message: "「测试账户」的额度仅剩 5%。",
    accountId: "a1",
    providerId: "p1",
    createdAt: Date.now() - 60_000,
    readAt: null,
  },
  {
    id: "n2",
    kind: "reset_approaching",
    severity: "INFO",
    title: "额度即将重置",
    message: "「测试账户」的额度将在约 30 分钟后重置。",
    accountId: "a1",
    providerId: "p1",
    createdAt: Date.now() - 120_000,
    readAt: Date.now() - 30_000,
  },
];

function makeBridge(overrides: Partial<DashboardBridge> = {}): DashboardBridge {
  return {
    listAccounts: vi.fn(async () => []),
    accountTargetIsLan: vi.fn(async () => false),
    createAccount: vi.fn(
      async (input: GenericOpenAiAccountInput): Promise<AccountViewModel> => ({
        id: "created",
        providerId: input.providerId,
        providerName: "测试供应商",
        label: input.label,
        status: "healthy",
        freshness: "unknown",
        snapshots: [],
      }),
    ),
    deleteAccount: vi.fn(async () => {}),
    refresh: vi.fn(async () => []),
    checkForUpdate: vi.fn(async () => null),
    installUpdate: vi.fn(async () => {}),
    listNotifications: vi.fn(async () => structuredClone(items)),
    markNotificationsRead: vi.fn(async () => {}),
    getNotificationPreferences: vi.fn(async () => structuredClone(prefs)),
    setNotificationPreferences: vi.fn(async () => {}),
    historySummary: vi.fn(async () => ({
      days: [],
      totalTokens: 0,
      totalCost: null,
      currency: null,
      activeDays: 0,
      rangeDays: 182,
    })),
    historyBreakdown: vi.fn(async () => []),
    providerMatrix: vi.fn(async () => []),
    manifestProfiles: vi.fn(async () => ({ profiles: [], skipped: [] })),
    usageWindows: vi.fn(async () => ({
      hours2: { tokens: 0, cost: null, currency: null, hasData: false },
      hours24: { tokens: 0, cost: null, currency: null, hasData: false },
      days30: { tokens: 0, cost: null, currency: null, hasData: false },
    })),
    ...overrides,
  };
}

describe("NotificationSection", () => {
  it("renders notification titles, messages, and severity labels", async () => {
    render(<NotificationSection bridge={makeBridge()} reloadSignal={0} />);
    expect(await screen.findByText("额度紧张")).toBeInTheDocument();
    expect(screen.getByText(/仅剩 5%/)).toBeInTheDocument();
    expect(screen.getByText("紧急")).toBeInTheDocument();
    expect(screen.getByText("信息")).toBeInTheDocument();
  });

  it("shows the unread count and marks all notifications read", async () => {
    const bridge = makeBridge();
    render(<NotificationSection bridge={bridge} reloadSignal={0} />);
    expect(await screen.findByText("1 条未读")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "标记所有通知为已读" }));
    await waitFor(() =>
      expect(bridge.markNotificationsRead).toHaveBeenCalledWith(),
    );
  });

  it("marks a single notification read by id", async () => {
    const bridge = makeBridge();
    render(<NotificationSection bridge={bridge} reloadSignal={0} />);
    const buttons = await screen.findAllByRole("button", { name: /标记通知已读/ });
    fireEvent.click(buttons[0]);
    await waitFor(() =>
      expect(bridge.markNotificationsRead).toHaveBeenCalledWith(["n1"]),
    );
  });

  it("renders the empty state when there are no notifications", async () => {
    const bridge = makeBridge({ listNotifications: vi.fn(async () => []) });
    render(<NotificationSection bridge={bridge} reloadSignal={0} />);
    expect(await screen.findByText("暂无通知，一切正常。")).toBeInTheDocument();
  });

  it("persists preference toggles immediately", async () => {
    const bridge = makeBridge();
    render(<NotificationSection bridge={bridge} reloadSignal={0} />);
    await screen.findByText("额度紧张");
    fireEvent.click(screen.getByLabelText(/^启用通知/));
    await waitFor(() =>
      expect(bridge.setNotificationPreferences).toHaveBeenCalledWith(
        expect.objectContaining({ enabled: false }),
      ),
    );
  });

  it("commits number preferences on blur", async () => {
    const bridge = makeBridge();
    render(<NotificationSection bridge={bridge} reloadSignal={0} />);
    await screen.findByText("额度紧张");
    const threshold = screen.getByLabelText(/低额度阈值/);
    fireEvent.change(threshold, { target: { value: "15" } });
    fireEvent.blur(threshold);
    await waitFor(() =>
      expect(bridge.setNotificationPreferences).toHaveBeenCalledWith(
        expect.objectContaining({ lowQuotaThreshold: 15 }),
      ),
    );
  });

  it("hides the settings form when preferences cannot be loaded", async () => {
    const bridge = makeBridge({
      getNotificationPreferences: vi.fn(async () => {
        throw new Error("offline");
      }),
    });
    render(<NotificationSection bridge={bridge} reloadSignal={0} />);
    expect(await screen.findByText("额度紧张")).toBeInTheDocument();
    expect(screen.queryByText("通知设置")).not.toBeInTheDocument();
  });

  it("shows an error and keeps editing when saving preferences fails", async () => {
    const bridge = makeBridge({
      setNotificationPreferences: vi.fn(async () => {
        throw new Error("rejected");
      }),
    });
    render(<NotificationSection bridge={bridge} reloadSignal={0} />);
    await screen.findByText("额度紧张");
    fireEvent.click(screen.getByLabelText(/^启用通知/));
    expect(await screen.findByRole("alert")).toHaveTextContent("保存失败");
  });
});
