import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";

const { mockListAccounts, mockCreateAccount, mockManifestProfiles, mockIsLan, mockListen } =
  vi.hoisted(() => ({
    mockListAccounts: vi.fn(),
    mockCreateAccount: vi.fn(),
    mockManifestProfiles: vi.fn(),
    mockIsLan: vi.fn(),
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
    accountTargetIsLan: mockIsLan,
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
    manifestProfiles: mockManifestProfiles,
    usageWindows: vi.fn(),
  },
}));

import { App } from "../features/dashboard/App";

const createdAccount = {
  id: "acc-1",
  providerId: "generic.openai_compatible:acc-1",
  providerName: "OpenAI 兼容",
  label: "Local gateway",
  status: "healthy" as const,
  freshness: "unknown" as const,
  snapshots: [],
};

async function fillForm(baseUrl: string) {
  mockListAccounts.mockResolvedValue([]);
  mockManifestProfiles.mockResolvedValue({ profiles: [], skipped: [] });
  render(<App />);
  await waitFor(() =>
    expect(screen.getByRole("button", { name: /添加账户|Add Provider/i })).toBeInTheDocument(),
  );
  fireEvent.click(screen.getByRole("button", { name: /添加账户|Add Provider/i }));
  await waitFor(() => expect(screen.getByLabelText(/账户名称|Account name/i)).toBeInTheDocument());

  fireEvent.change(screen.getByLabelText(/账户名称|Account name/i), {
    target: { value: "Local gateway" },
  });
  fireEvent.change(screen.getByLabelText(/接口地址|Base URL/i), { target: { value: baseUrl } });
  fireEvent.change(screen.getByLabelText(/API 密钥|API key/i), { target: { value: "sk-test" } });
}

function submitForm() {
  const form = screen
    .getByRole("button", { name: /连接账户|Connect/i })
    .closest("form");
  expect(form).not.toBeNull();
  fireEvent.submit(form!);
}

describe("LAN consent before account creation", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockCreateAccount.mockResolvedValue(createdAccount);
  });

  it("creates a public account without asking", async () => {
    mockIsLan.mockResolvedValue(false);
    await fillForm("https://api.example.com/v1");
    submitForm();

    await waitFor(() => expect(mockCreateAccount).toHaveBeenCalledTimes(1));
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
  });

  it("asks before creating an account that points at the LAN", async () => {
    mockIsLan.mockResolvedValue(true);
    await fillForm("http://localhost:4000/v1");
    submitForm();

    const dialog = await waitFor(() => screen.getByRole("alertdialog"));
    expect(dialog).toHaveTextContent(/局域网|LAN/i);
    // Nothing is created until the user confirms.
    expect(mockCreateAccount).not.toHaveBeenCalled();
  });

  it("creates the account once the LAN prompt is confirmed", async () => {
    mockIsLan.mockResolvedValue(true);
    await fillForm("http://192.168.1.10:8080/v1");
    submitForm();
    await waitFor(() => screen.getByRole("alertdialog"));

    fireEvent.click(screen.getByRole("button", { name: /我确认，继续|Continue/i }));

    await waitFor(() => expect(mockCreateAccount).toHaveBeenCalledTimes(1));
    expect(mockCreateAccount.mock.calls[0][0].baseUrl).toBe("http://192.168.1.10:8080/v1");
  });

  it("abandons creation when the LAN prompt is cancelled", async () => {
    mockIsLan.mockResolvedValue(true);
    await fillForm("http://10.0.0.5/v1");
    submitForm();
    await waitFor(() => screen.getByRole("alertdialog"));

    const cancel = screen
      .getAllByRole("button", { name: /取消|Cancel/i })
      .find((button) => button.closest('[role="alertdialog"]'));
    fireEvent.click(cancel!);

    await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
    expect(mockCreateAccount).not.toHaveBeenCalled();
  });
});
