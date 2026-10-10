import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, act, waitFor } from "@testing-library/react";

const { mockListAccounts, mockRefresh, mockCreateAccount, mockDeleteAccount, mockListen } = vi.hoisted(() => ({
  mockListAccounts: vi.fn(),
  mockRefresh: vi.fn(),
  mockCreateAccount: vi.fn(),
  mockDeleteAccount: vi.fn(),
  mockListen: vi.fn(() => new Promise(() => {})),
}));

vi.mock("@lobehub/icons", () => ({
  ProviderIcon: () => null,
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: mockListen,
}));

vi.mock("../../lib/tauri", () => ({
  shell: {
    openDashboard: vi.fn().mockResolvedValue(undefined),
    showBubble: vi.fn().mockResolvedValue(undefined),
    persistBubblePosition: vi.fn().mockResolvedValue(undefined),
  },
  tauriBridge: {
    listAccounts: mockListAccounts,
    createAccount: mockCreateAccount,
    deleteAccount: mockDeleteAccount,
    refresh: mockRefresh,
    checkForUpdate: vi.fn().mockResolvedValue(null),
    installUpdate: vi.fn(),
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
  source: "official_api" as const,
  lastUpdatedAt: "2026-10-10T12:00:00Z",
};

describe("Dashboard state machine", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("transitions from loading to ready on successful load", async () => {
    mockListAccounts.mockResolvedValue([mockAccount]);
    render(<App />);
    await waitFor(() => expect(screen.getByText("Test Account")).toBeInTheDocument());
    expect(screen.getByText("Test Account")).toBeInTheDocument();
  });

  it("transitions to offline on load failure", async () => {
    mockListAccounts.mockRejectedValue(new Error("network error"));
    render(<App />);
    await waitFor(() => expect(screen.getByText(/加载失败|offline/i)).toBeInTheDocument());
    expect(screen.getByText(/加载失败|offline/i)).toBeInTheDocument();
  });

  it("shows empty state when no accounts", async () => {
    mockListAccounts.mockResolvedValue([]);
    render(<App />);
    await waitFor(() => expect(screen.getByText(/添加账户|Add Provider/i)).toBeInTheDocument());
    expect(screen.getByText(/添加账户|Add Provider/i)).toBeInTheDocument();
  });

  it("handles refresh success", async () => {
    mockListAccounts.mockResolvedValue([mockAccount]);
    mockRefresh.mockResolvedValue([{ ...mockAccount, status: "healthy" }]);
    render(<App />);
    await act(async () => {});
    const refreshButton = screen.getByLabelText(/刷新|Refresh/i);
    fireEvent.click(refreshButton);
    await waitFor(() => expect(mockRefresh).toHaveBeenCalled());
    expect(mockRefresh).toHaveBeenCalled();
  });

  it("handles refresh failure with error notice", async () => {
    mockListAccounts.mockResolvedValue([mockAccount]);
    mockRefresh.mockRejectedValue(new Error("refresh failed"));
    render(<App />);
    await act(async () => {});
    const refreshButton = screen.getByLabelText(/刷新|Refresh/i);
    fireEvent.click(refreshButton);
    await waitFor(() => expect(screen.getByText(/刷新失败|failed/i)).toBeInTheDocument());
    expect(screen.getByText(/刷新失败|failed/i)).toBeInTheDocument();
  });

  it("filters accounts by attention", async () => {
    const healthyAccount = { ...mockAccount, id: "acc-1", status: "healthy" as const };
    const errorAccount = { ...mockAccount, id: "acc-2", status: "error" as const, label: "Error Account" };
    mockListAccounts.mockResolvedValue([healthyAccount, errorAccount]);
    render(<App />);
    await waitFor(() => expect(screen.getByText("Test Account")).toBeInTheDocument());
    expect(screen.getByText("Test Account")).toBeInTheDocument();
    expect(screen.getByText("Error Account")).toBeInTheDocument();
    const attentionFilter = screen.getByRole("button", { name: /注意|Attention/i });
    fireEvent.click(attentionFilter);
    await waitFor(() => expect(screen.queryByText("Test Account")).not.toBeInTheDocument());
    expect(screen.queryByText("Test Account")).not.toBeInTheDocument();
    expect(screen.getByText("Error Account")).toBeInTheDocument();
  });

  it("handles delete success", async () => {
    mockListAccounts.mockResolvedValue([mockAccount]);
    mockDeleteAccount.mockResolvedValue(undefined);
    render(<App />);
    await act(async () => {});
    const deleteButton = screen.getByLabelText(/移除|Delete/i);
    fireEvent.click(deleteButton);
    await act(async () => {});
    const confirmButton = screen.getByRole("button", { name: /确认|Confirm/i });
    fireEvent.click(confirmButton);
    await waitFor(() => expect(mockDeleteAccount).toHaveBeenCalledWith("acc-1"));
    expect(mockDeleteAccount).toHaveBeenCalledWith("acc-1");
  });

  it("shows form when add provider clicked", async () => {
    mockListAccounts.mockResolvedValue([]);
    render(<App />);
    await act(async () => {});
    const addButton = screen.getByRole("button", { name: /添加|Add/i });
    fireEvent.click(addButton);
    await waitFor(() => expect(screen.getByText(/创建账户|Add Provider/i)).toBeInTheDocument());
    expect(screen.getByText(/创建账户|Add Provider/i)).toBeInTheDocument();
  });
});
