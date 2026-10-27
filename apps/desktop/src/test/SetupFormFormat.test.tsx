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
    accountTargetIsLan: vi.fn(async () => false),
    lanConsentBegin: vi.fn().mockResolvedValue(null),
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

/** One profile per supported wire format. */
const openaiProfile = {
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
const anthropicProfile = {
  id: "anthropic",
  name: "Anthropic",
  baseUrl: "https://api.anthropic.com/v1",
  apiFormat: "anthropic",
  customBaseUrl: false,
  level: "L2",
  accountTypes: ["api"],
  quotaUnits: ["token"],
  targetWindows: ["provider-defined"],
  endpoints: { models: "/v1/models" },
};
const geminiProfile = {
  id: "gemini",
  name: "Google Gemini",
  baseUrl: "https://generativelanguage.googleapis.com/v1beta",
  apiFormat: "google-ai",
  customBaseUrl: false,
  level: "L2",
  accountTypes: ["api"],
  quotaUnits: ["token"],
  targetWindows: ["provider-defined"],
  endpoints: { models: "/v1beta/models" },
};

async function openSetupForm() {
  mockListAccounts.mockResolvedValue([]);
  mockManifestProfiles.mockResolvedValue({
    profiles: [openaiProfile, anthropicProfile, geminiProfile],
    skipped: [],
  });
  render(<App />);
  await waitFor(() =>
    expect(screen.getByRole("button", { name: /添加账户|Add Provider/i })).toBeInTheDocument(),
  );
  fireEvent.click(screen.getByRole("button", { name: /添加账户|Add Provider/i }));
  await waitFor(() => expect(screen.getByLabelText(/账户名称|Account name/i)).toBeInTheDocument());
  await waitFor(() => expect(screen.getByRole("button", { name: /DeepSeek/i })).toBeInTheDocument());
}

describe("setup form brand variant grouping", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  const zhipuProfile = {
    id: "zhipu",
    name: "Zhipu / GLM",
    baseUrl: "https://open.bigmodel.cn/api/paas/v4",
    apiFormat: "openai-compatible",
    customBaseUrl: false,
    level: "L2",
    accountTypes: ["api"],
    quotaUnits: ["quota"],
    targetWindows: ["custom"],
    endpoints: { models: "/models" },
    brand: "zhipu",
    variant: "国内版",
  };
  const zhipuIntlProfile = {
    ...zhipuProfile,
    id: "zhipu-international",
    name: "Zhipu / GLM 国际版",
    baseUrl: "https://api.z.ai/api/paas/v4",
    variant: "国际版",
  };

  async function openWithZhipuPair() {
    mockListAccounts.mockResolvedValue([]);
    mockManifestProfiles.mockResolvedValue({
      profiles: [zhipuProfile, zhipuIntlProfile],
      skipped: [],
    });
    render(<App />);
    await waitFor(() =>
      expect(screen.getByRole("button", { name: /添加账户|Add Provider/i })).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByRole("button", { name: /添加账户|Add Provider/i }));
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Zhipu / GLM" })).toBeInTheDocument(),
    );
  }

  it("collapses platform variants into one capsule with an ellipsis expander", async () => {
    await openWithZhipuPair();
    // Only ONE capsule for the brand: the two manifests share it, and the
    // ellipsis expander is a separate control with its own label.
    expect(screen.getByRole("button", { name: "Zhipu / GLM" })).toBeInTheDocument();
    const more = screen.getByRole("button", { name: "Zhipu / GLM 的其他平台版本" });

    fireEvent.click(more);
    const option = screen.getByRole("option", { name: /国际版/ });
    expect(option).toBeInTheDocument();

    fireEvent.click(option);
    await waitFor(() =>
      expect(screen.getByLabelText(/接口地址|Base URL/i)).toHaveValue("https://api.z.ai/api/paas/v4"),
    );
    // The capsule face now names the selected platform variant.
    expect(screen.getByRole("button", { name: "Zhipu / GLM 国际版" })).toBeInTheDocument();
  });
});

describe("setup form API format categorization", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders the picker grouped by the three wire formats", async () => {
    await openSetupForm();
    expect(screen.getByText("OpenAI 兼容格式")).toBeInTheDocument();
    expect(screen.getByText("Anthropic 格式")).toBeInTheDocument();
    expect(screen.getByText("Gemini 格式")).toBeInTheDocument();
    expect(screen.getByText("自定义接入")).toBeInTheDocument();
    // Each native profile is selectable under its own group.
    expect(screen.getByRole("button", { name: /Google Gemini/i })).toBeEnabled();
  });

  it("switching the custom format swaps the default host and hides probe endpoints", async () => {
    await openSetupForm();
    fireEvent.click(screen.getByRole("button", { name: /自定义 \/ 兼容/i }));

    // OpenAI custom keeps the probe endpoint fields.
    expect(screen.getByLabelText(/额度接口/i)).toBeInTheDocument();

    fireEvent.click(screen.getByRole("radio", { name: "Anthropic" }));
    await waitFor(() =>
      expect(screen.getByLabelText(/接口地址/i)).toHaveValue("https://api.anthropic.com/v1"),
    );
    expect(screen.queryByLabelText(/额度接口/i)).not.toBeInTheDocument();
    expect(screen.getByText(/由原生适配器探测/i)).toBeInTheDocument();

    fireEvent.click(screen.getByRole("radio", { name: "Gemini" }));
    await waitFor(() =>
      expect(screen.getByLabelText(/接口地址/i)).toHaveValue(
        "https://generativelanguage.googleapis.com/v1beta",
      ),
    );
  });

  it("submits the chosen custom format to the backend", async () => {
    mockCreateAccount.mockResolvedValue({ id: "acc-1" });
    await openSetupForm();
    fireEvent.click(screen.getByRole("button", { name: /自定义 \/ 兼容/i }));
    fireEvent.click(screen.getByRole("radio", { name: "Anthropic" }));

    fireEvent.change(await waitFor(() => screen.getByLabelText(/账户名称|Account name/i)), {
      target: { value: "My relay" },
    });
    fireEvent.change(screen.getByLabelText(/API 密钥|API key/i), { target: { value: "sk-test" } });
    fireEvent.click(screen.getByRole("button", { name: /连接|Connect|添加|Add/i }));

    await waitFor(() => expect(mockCreateAccount).toHaveBeenCalled());
    const submitted = mockCreateAccount.mock.calls[0][0];
    expect(submitted.apiFormat).toBe("anthropic");
    expect(submitted.baseUrl).toBe("https://api.anthropic.com/v1");
    expect(submitted.quotaEndpoint).toBeUndefined();
  });
});
