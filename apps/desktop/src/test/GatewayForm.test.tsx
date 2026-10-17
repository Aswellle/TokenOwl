import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";

const { mockListAccounts, mockCreateAccount, mockManifestProfiles, mockListen } = vi.hoisted(() => ({
  mockListAccounts: vi.fn(),
  mockCreateAccount: vi.fn(),
  mockManifestProfiles: vi.fn(),
  mockListen: vi.fn(() => new Promise(() => {})),
}));

vi.mock("@lobehub/icons", () => ({
  ProviderIcon: () => null,
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: mockListen,
}));

vi.mock("../lib/tauri", () => ({
  isTauri: vi.fn(() => false),
  shell: {
    openDashboard: vi.fn().mockResolvedValue(undefined),
    showBubble: vi.fn().mockResolvedValue(undefined),
    persistBubblePosition: vi.fn().mockResolvedValue(undefined),
    openProjectPage: vi.fn().mockResolvedValue(undefined),
  },
  tauriBridge: {
    listAccounts: mockListAccounts,
    createAccount: mockCreateAccount,
    deleteAccount: vi.fn().mockResolvedValue(undefined),
    refresh: vi.fn().mockResolvedValue([]),
    checkForUpdate: vi.fn().mockResolvedValue(null),
    installUpdate: vi.fn(),
    listNotifications: vi.fn().mockResolvedValue([]),
    markNotificationsRead: vi.fn().mockResolvedValue(undefined),
    getNotificationPreferences: vi.fn(),
    setNotificationPreferences: vi.fn().mockResolvedValue(undefined),
    historySummary: vi.fn(),
    historyBreakdown: vi.fn().mockResolvedValue([]),
    providerMatrix: vi.fn().mockResolvedValue([]),
    manifestProfiles: mockManifestProfiles,
    usageWindows: vi.fn(),
  },
}));

import { App } from "../features/dashboard/App";

/** A gateway profile: the manifest holds only a placeholder host. */
const gatewayProfile = {
  id: "litellm",
  name: "LiteLLM",
  baseUrl: "https://your-gateway.example",
  apiFormat: "openai-compatible",
  customBaseUrl: true,
  level: "L2",
  accountTypes: ["gateway"],
  quotaUnits: ["custom"],
  targetWindows: ["custom"],
  endpoints: { models: "/v1/models" },
};

/** A fixed-URL built-in: the manifest host is authoritative. */
const fixedProfile = {
  id: "deepseek",
  name: "DeepSeek",
  baseUrl: "https://api.deepseek.com/v1",
  apiFormat: "openai-compatible",
  customBaseUrl: false,
  level: "L2",
  accountTypes: ["api"],
  quotaUnits: ["balance"],
  targetWindows: ["custom"],
  endpoints: { models: "/models", balance: "/user/balance" },
};

async function openSetupForm() {
  mockListAccounts.mockResolvedValue([]);
  mockManifestProfiles.mockResolvedValue({
    profiles: [fixedProfile, gatewayProfile],
    skipped: [],
  });
  render(<App />);
  await waitFor(() =>
    expect(screen.getByRole("button", { name: /添加账户|Add Provider/i })).toBeInTheDocument(),
  );
  fireEvent.click(screen.getByRole("button", { name: /添加账户|Add Provider/i }));
  await waitFor(() => expect(screen.getByLabelText(/账户名称|Account name/i)).toBeInTheDocument());
  // Profiles arrive from the bridge after the form mounts; the picker is
  // empty until then, so wait for a profile button before selecting one.
  await waitFor(() => expect(screen.getByRole("button", { name: /LiteLLM/i })).toBeInTheDocument());
}

function baseUrlInput(): HTMLInputElement {
  return screen.getByLabelText(/网关地址|Gateway address|接口地址|Base URL/i) as HTMLInputElement;
}

describe("setup form base URL handling", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("keeps a fixed-URL built-in read-only and prefilled from the manifest", async () => {
    await openSetupForm();
    fireEvent.click(screen.getByRole("button", { name: /DeepSeek/i }));
    await waitFor(() => expect(baseUrlInput().value).toBe("https://api.deepseek.com/v1"));
    expect(baseUrlInput()).toHaveAttribute("readonly");
  });

  it("lets a gateway profile be typed into instead of reverting to the placeholder", async () => {
    await openSetupForm();
    fireEvent.click(screen.getByRole("button", { name: /LiteLLM/i }));

    const input = await waitFor(() => {
      const field = baseUrlInput();
      expect(field).not.toHaveAttribute("readonly");
      return field;
    });

    // The manifest placeholder must not be the field's value, and typing has
    // to survive a re-render instead of snapping back to it.
    expect(input.value).not.toContain("your-gateway.example");
    fireEvent.change(input, { target: { value: "http://localhost:4000/v1" } });
    expect(baseUrlInput().value).toBe("http://localhost:4000/v1");
  });

  it("submits the gateway URL the user typed, never a placeholder", async () => {
    mockCreateAccount.mockResolvedValue({ id: "acc-1" });
    await openSetupForm();
    fireEvent.click(screen.getByRole("button", { name: /LiteLLM/i }));

    fireEvent.change(await waitFor(() => screen.getByLabelText(/账户名称|Account name/i)), {
      target: { value: "My gateway" },
    });
    fireEvent.change(baseUrlInput(), { target: { value: "https://gateway.mine.test/v1" } });
    fireEvent.change(screen.getByLabelText(/API 密钥|API key/i), {
      target: { value: "sk-test" },
    });
    fireEvent.click(screen.getByRole("button", { name: /连接|Connect|添加|Add/i }));

    await waitFor(() => expect(mockCreateAccount).toHaveBeenCalled());
    const submitted = mockCreateAccount.mock.calls[0][0];
    expect(submitted.providerId).toBe("litellm");
    expect(submitted.baseUrl).toBe("https://gateway.mine.test/v1");
    expect(submitted.baseUrl).not.toContain("your-gateway.example");
    expect(submitted.baseUrl).not.toContain("api.openai.com");
  });
});
