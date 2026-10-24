import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";

const {
  mockListAccounts,
  mockCreateAccount,
  mockManifestProfiles,
  mockLanConsentBegin,
  mockIsLan,
  mockListen,
} = vi.hoisted(() => ({
  mockListAccounts: vi.fn(),
  mockCreateAccount: vi.fn(),
  mockManifestProfiles: vi.fn(),
  mockLanConsentBegin: vi.fn(),
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
    lanConsentBegin: mockLanConsentBegin,
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

function consentCancelButton() {
  return screen
    .getAllByRole("button", { name: /取消|Cancel/i })
    .find((button) => button.closest('[role="alertdialog"]'))!;
}

describe("LAN consent before account creation", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockCreateAccount.mockResolvedValue(createdAccount);
  });

  it("creates a public account without asking", async () => {
    mockLanConsentBegin.mockResolvedValue(null);
    await fillForm("https://api.example.com/v1");
    submitForm();

    await waitFor(() => expect(mockCreateAccount).toHaveBeenCalledTimes(1));
    expect(mockCreateAccount.mock.calls[0][0].lanConsentToken).toBeUndefined();
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
  });

  it("asks before creating an account that points at the LAN", async () => {
    mockLanConsentBegin.mockResolvedValue("token-lan-1");
    await fillForm("https://localhost:4000/v1");
    submitForm();

    const dialog = await waitFor(() => screen.getByRole("alertdialog"));
    expect(dialog).toHaveTextContent(/局域网|LAN/i);
    expect(dialog).toHaveAttribute("aria-modal", "true");
    // Nothing is created until the user confirms.
    expect(mockCreateAccount).not.toHaveBeenCalled();
  });

  it("passes the one-time consent token when confirmed", async () => {
    mockLanConsentBegin.mockResolvedValue("token-lan-2");
    await fillForm("https://192.168.1.10:8080/v1");
    submitForm();
    await waitFor(() => screen.getByRole("alertdialog"));

    fireEvent.click(screen.getByRole("button", { name: /我确认，继续|Continue/i }));

    await waitFor(() => expect(mockCreateAccount).toHaveBeenCalledTimes(1));
    expect(mockCreateAccount.mock.calls[0][0].baseUrl).toBe("https://192.168.1.10:8080/v1");
    expect(mockCreateAccount.mock.calls[0][0].lanConsentToken).toBe("token-lan-2");
  });

  it("abandons creation when the LAN prompt is cancelled", async () => {
    mockLanConsentBegin.mockResolvedValue("token-lan-3");
    await fillForm("https://10.0.0.5/v1");
    submitForm();
    await waitFor(() => screen.getByRole("alertdialog"));

    fireEvent.click(consentCancelButton());

    await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
    expect(mockCreateAccount).not.toHaveBeenCalled();
  });

  it("cancels on Escape without creating", async () => {
    mockLanConsentBegin.mockResolvedValue("token-lan-4");
    await fillForm("https://10.0.0.6/v1");
    submitForm();
    const dialog = await waitFor(() => screen.getByRole("alertdialog"));

    fireEvent.keyDown(dialog, { key: "Escape" });

    await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
    expect(mockCreateAccount).not.toHaveBeenCalled();
  });

  /// A submit event (Enter key) while the dialog is open must never replay
  /// into a confirmed creation — confirmation comes from the button only.
  it("does not create when the form is submitted while the dialog is open", async () => {
    mockLanConsentBegin.mockResolvedValue("token-lan-5");
    await fillForm("https://10.0.0.7/v1");
    submitForm();
    await waitFor(() => screen.getByRole("alertdialog"));

    submitForm();
    submitForm();

    expect(mockCreateAccount).not.toHaveBeenCalled();
    expect(screen.getByRole("alertdialog")).toBeInTheDocument();
  });

  /// The classifier command failing must fail closed: a recoverable error is
  /// shown and nothing is created — never "unknown means public".
  it("fails closed with a visible error when classification fails", async () => {
    mockLanConsentBegin.mockRejectedValue(new Error("ipc unavailable"));
    await fillForm("https://10.0.0.8/v1");
    submitForm();

    await waitFor(() => expect(screen.getByRole("alert")).toBeInTheDocument());
    expect(mockCreateAccount).not.toHaveBeenCalled();
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
  });

  /// Slow IPC must not let a second click race into two creations.
  it("ignores a second submit while classification is in flight", async () => {
    let release: (() => void) | undefined;
    mockLanConsentBegin.mockImplementation(
      () => new Promise<null>((resolve) => {
        release = () => resolve(null);
      }),
    );
    await fillForm("https://api.example.com/v1");
    submitForm();
    submitForm();
    expect(mockLanConsentBegin).toHaveBeenCalledTimes(1);
    release?.();
    await waitFor(() => expect(mockCreateAccount).toHaveBeenCalledTimes(1));
  });
});
