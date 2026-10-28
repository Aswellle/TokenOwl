import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";

const { mockListAccounts, mockCreateAccount, mockManifestProfiles, mockListen } = vi.hoisted(() => ({
  mockListAccounts: vi.fn(),
  mockCreateAccount: vi.fn(),
  mockManifestProfiles: vi.fn(),
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
    accountTargetIsLan: vi.fn(async () => false),
    lanConsentBegin: vi.fn().mockResolvedValue(null),
    createAccount: mockCreateAccount,
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

async function openSetupForm() {
  mockListAccounts.mockResolvedValue([]);
  mockManifestProfiles.mockResolvedValue({ profiles: [], skipped: [] });
  render(<App />);
  await waitFor(() =>
    expect(screen.getByRole("button", { name: /添加账户|Add Provider/i })).toBeInTheDocument(),
  );
  fireEvent.click(screen.getByRole("button", { name: /添加账户|Add Provider/i }));
  await waitFor(() => expect(screen.getByLabelText(/账户名称|Account name/i)).toBeInTheDocument());
}

/**
 * The setup panel used to render as a `<section>`, so its `type="submit"`
 * button had no form owner and activation did nothing at all — account
 * creation through the UI was silently impossible. These tests pin the panel
 * to a real `<form>` so the submit path stays reachable.
 */
describe("setup form submit wiring", () => {
  it("exposes the connect button as a submit control inside a real form", async () => {
    await openSetupForm();
    const button = screen.getByRole("button", { name: /连接账户|Connect/i });
    expect(button).toHaveAttribute("type", "submit");
    expect(button.closest("form")).not.toBeNull();
  });

  it("creates the account through the form's submit path", async () => {
    mockCreateAccount.mockResolvedValue({
      id: "acc-1",
      providerId: "generic.openai_compatible:acc-1",
      providerName: "OpenAI 兼容",
      label: "Probe",
      status: "healthy",
      freshness: "unknown",
      snapshots: [],
    });
    await openSetupForm();

    fireEvent.change(screen.getByLabelText(/账户名称|Account name/i), {
      target: { value: "Probe" },
    });
    fireEvent.change(screen.getByLabelText(/接口地址|Base URL/i), {
      target: { value: "https://probe.test/v1" },
    });
    fireEvent.change(screen.getByLabelText(/API 密钥|API key/i), {
      target: { value: "sk-probe" },
    });
    fireEvent.submit(screen.getByRole("button", { name: /连接账户|Connect/i }).closest("form")!);

    await waitFor(() => expect(mockCreateAccount).toHaveBeenCalledTimes(1));
    expect(mockCreateAccount.mock.calls[0][0]).toMatchObject({
      label: "Probe",
      baseUrl: "https://probe.test/v1",
      apiKey: "sk-probe",
    });
    // Success is confirmed next to the submit button and the form resets so
    // the next account starts clean.
    expect(await screen.findByText(/账户已连接/)).toBeInTheDocument();
    expect(screen.getByLabelText(/账户名称|Account name/i)).toHaveValue("");
    expect(screen.getByLabelText(/API 密钥|API key/i)).toHaveValue("");
  });

  it("reports a failure instead of silently doing nothing", async () => {
    mockCreateAccount.mockRejectedValue(new Error("boom"));
    await openSetupForm();

    fireEvent.change(screen.getByLabelText(/账户名称|Account name/i), {
      target: { value: "Probe" },
    });
    fireEvent.change(screen.getByLabelText(/接口地址|Base URL/i), {
      target: { value: "https://probe.test/v1" },
    });
    fireEvent.change(screen.getByLabelText(/API 密钥|API key/i), {
      target: { value: "sk-probe" },
    });
    fireEvent.submit(screen.getByRole("button", { name: /连接账户|Connect/i }).closest("form")!);

    await waitFor(() => expect(screen.getByRole("alert")).toBeInTheDocument());
    // A failed attempt must not wipe the secret: the error tells the user to
    // check the key, so the key they typed has to still be there.
    expect(screen.getByLabelText(/API 密钥|API key/i)).toHaveValue("sk-probe");
    expect(screen.getByLabelText(/账户名称|Account name/i)).toHaveValue("Probe");
  });

  it("rejects an empty form with an inline error and moves focus to the name field", async () => {
    await openSetupForm();
    fireEvent.submit(screen.getByRole("button", { name: /连接账户|Connect/i }).closest("form")!);

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent(/请填写账户名称与接口地址/);
    expect(mockCreateAccount).not.toHaveBeenCalled();
    expect(screen.getByLabelText(/账户名称|Account name/i)).toHaveFocus();
  });

  it("rejects a malformed base URL with a fix-it message", async () => {
    await openSetupForm();
    fireEvent.change(screen.getByLabelText(/账户名称|Account name/i), {
      target: { value: "Probe" },
    });
    fireEvent.change(screen.getByLabelText(/接口地址|Base URL/i), {
      target: { value: "not a url" },
    });
    fireEvent.change(screen.getByLabelText(/API 密钥|API key/i), {
      target: { value: "sk-probe" },
    });
    fireEvent.submit(screen.getByRole("button", { name: /连接账户|Connect/i }).closest("form")!);

    expect(await screen.findByRole("alert")).toHaveTextContent(/完整 URL/);
    expect(screen.getByLabelText(/接口地址|Base URL/i)).toHaveFocus();
    expect(mockCreateAccount).not.toHaveBeenCalled();
  });

  it("asks for the API key before touching the bridge", async () => {
    await openSetupForm();
    fireEvent.change(screen.getByLabelText(/账户名称|Account name/i), {
      target: { value: "Probe" },
    });
    fireEvent.change(screen.getByLabelText(/接口地址|Base URL/i), {
      target: { value: "https://probe.test/v1" },
    });
    fireEvent.submit(screen.getByRole("button", { name: /连接账户|Connect/i }).closest("form")!);

    expect(await screen.findByRole("alert")).toHaveTextContent(/API 密钥/);
    expect(screen.getByLabelText(/API 密钥|API key/i)).toHaveFocus();
    expect(mockCreateAccount).not.toHaveBeenCalled();
  });
});
