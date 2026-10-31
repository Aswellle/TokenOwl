import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";

const {
  mockListAccounts,
  mockRefresh,
  mockCreateAccount,
  mockDeleteAccount,
  mockManifestProfiles,
  mockLanConsentBegin,
  mockListen,
} = vi.hoisted(() => ({
  mockListAccounts: vi.fn(),
  mockRefresh: vi.fn(),
  mockCreateAccount: vi.fn(),
  mockDeleteAccount: vi.fn(),
  mockManifestProfiles: vi.fn(),
  mockLanConsentBegin: vi.fn(),
  mockListen: vi.fn(() => new Promise(() => {})),
}));

vi.mock("@lobehub/icons", () => ({ ProviderIcon: () => null }));
vi.mock("@tauri-apps/api/event", () => ({ listen: mockListen }));
vi.mock("../lib/tauri", () => ({
  isTauri: vi.fn(() => false),
  shell: {
    openDashboard: vi.fn(),
    showBubble: vi.fn(),
    persistBubblePosition: vi.fn(),
    openProjectPage: vi.fn(),
  },
  tauriBridge: {
    listAccounts: mockListAccounts,
    createAccount: mockCreateAccount,
    accountTargetIsLan: vi.fn().mockResolvedValue(false),
    lanConsentBegin: mockLanConsentBegin,
    deleteAccount: mockDeleteAccount,
    refresh: mockRefresh,
    checkForUpdate: vi.fn(),
    installUpdate: vi.fn(),
    listNotifications: vi.fn().mockResolvedValue([]),
    markNotificationsRead: vi.fn(),
    getNotificationPreferences: vi.fn(),
    setNotificationPreferences: vi.fn(),
    historySummary: vi.fn(),
    historyBreakdown: vi.fn().mockResolvedValue([]),
    providerMatrix: vi.fn().mockResolvedValue([]),
    manifestProfiles: mockManifestProfiles,
    usageWindows: vi.fn(),
  },
}));

import { App } from "../features/dashboard/App";

const mockAccount = {
  id: "acc-1",
  providerId: "openai",
  providerName: "OpenAI",
  label: "Test Account",
  status: "healthy" as const,
  freshness: "fresh" as const,
  snapshots: [],
  source: "OFFICIAL_API",
  lastUpdatedAt: 1_762_000_000,
};

describe("Dashboard state machine", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockManifestProfiles.mockResolvedValue({ profiles: [], skipped: [] });
    mockLanConsentBegin.mockResolvedValue(null);
  });

  it("transitions from loading to ready on successful load", async () => {
    mockListAccounts.mockResolvedValue([mockAccount]);
    render(<App />);
    await waitFor(() => expect(screen.getByText("Test Account")).toBeInTheDocument());
  });

  it("close light collapses to the bubble instead of plain hiding", async () => {
    const tauri = await import("../lib/tauri");
    vi.mocked(tauri.isTauri).mockReturnValue(true);
    vi.mocked(tauri.shell.showBubble).mockResolvedValue(undefined);
    mockListAccounts.mockResolvedValue([mockAccount]);
    render(<App />);
    await waitFor(() => expect(screen.getByText("Test Account")).toBeInTheDocument());
    fireEvent.click(screen.getByRole("button", { name: "关闭面板（回到 Bubble）" }));
    expect(tauri.shell.showBubble).toHaveBeenCalled();
  });

  it("transitions to offline on load failure", async () => {
    mockListAccounts.mockRejectedValue(new Error("network error"));
    render(<App />);
    // The offline copy appears in both the status chip and the empty state;
    // one of each is exactly the point.
    await waitFor(() =>
      expect(screen.getAllByText(/本地服务不可用|offline/i).length).toBeGreaterThan(0),
    );
  });

  it("shows empty state when no accounts", async () => {
    mockListAccounts.mockResolvedValue([]);
    render(<App />);
    await waitFor(() =>
      expect(screen.getByRole("button", { name: /添加账户|Add Provider/i })).toBeInTheDocument(),
    );
  });

  it("handles refresh success", async () => {
    mockListAccounts.mockResolvedValue([mockAccount]);
    mockRefresh.mockResolvedValue([{ ...mockAccount, status: "healthy" }]);
    render(<App />);
    await waitFor(() => expect(screen.getByText("Test Account")).toBeInTheDocument());

    fireEvent.click(screen.getByRole("button", { name: /全部刷新/ }));

    await waitFor(() => expect(mockRefresh).toHaveBeenCalled());
  });

  it("handles refresh failure with an actionable first-refresh notice", async () => {
    mockListAccounts.mockResolvedValue([mockAccount]);
    mockRefresh.mockRejectedValue(new Error("refresh failed"));
    render(<App />);
    await waitFor(() => expect(screen.getByText("Test Account")).toBeInTheDocument());

    fireEvent.click(screen.getByRole("button", { name: /全部刷新/ }));

    await waitFor(() =>
      expect(screen.getByText(/首次刷新失败/)).toBeInTheDocument(),
    );
  });

  it("filters accounts by the needs-attention chip", async () => {
    const healthyAccount = { ...mockAccount, id: "acc-1", status: "healthy" as const };
    const errorAccount = {
      ...mockAccount,
      id: "acc-2",
      status: "error" as const,
      label: "Error Account",
    };
    mockListAccounts.mockResolvedValue([healthyAccount, errorAccount]);
    render(<App />);
    await waitFor(() => expect(screen.getByText("Test Account")).toBeInTheDocument());
    expect(screen.getByText("Error Account")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /需处理/ }));

    await waitFor(() =>
      expect(screen.queryByText("Test Account")).not.toBeInTheDocument(),
    );
    expect(screen.getByText("Error Account")).toBeInTheDocument();
  });

  it("removes an account after the inline confirm", async () => {
    mockListAccounts.mockResolvedValue([mockAccount]);
    mockDeleteAccount.mockResolvedValue(undefined);
    render(<App />);
    await waitFor(() => expect(screen.getByText("Test Account")).toBeInTheDocument());

    fireEvent.click(screen.getByRole("button", { name: /移除账户 Test Account/ }));
    const confirm = await screen.findByRole("button", { name: /确认移除/ });
    fireEvent.click(confirm);

    await waitFor(() => expect(mockDeleteAccount).toHaveBeenCalledWith("acc-1"));
  });

  it("returns to the overview from a provider view on Escape", async () => {
    mockListAccounts.mockResolvedValue([mockAccount]);
    render(<App />);
    await waitFor(() => expect(screen.getByText("Test Account")).toBeInTheDocument());

    fireEvent.click(screen.getByRole("button", { name: /查看供应商/ }));
    await waitFor(() =>
      expect(screen.getByRole("heading", { name: "Test Account" })).toBeVisible(),
    );

    fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() => expect(screen.getByRole("heading", { name: "概览" })).toBeVisible());
  });

  it("shows the setup form when add provider is clicked", async () => {
    mockListAccounts.mockResolvedValue([]);
    render(<App />);
    await waitFor(() =>
      expect(screen.getByRole("button", { name: /添加账户|Add Provider/i })).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByRole("button", { name: /添加账户|Add Provider/i }));

    await waitFor(() =>
      expect(screen.getByLabelText(/账户名称|Account name/i)).toBeInTheDocument(),
    );
  });
});
