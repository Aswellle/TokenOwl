import type {
  AccountViewModel,
  DashboardBridge,
  GenericOpenAiAccountInput,
} from "../types";

/**
 * Demo data for `vite dev` in a plain browser, where the Tauri commands do not
 * exist. Never imported in production builds (guarded by import.meta.env.DEV at
 * the call site) and never wired to the packaged app.
 */

const minute = 60_000;
const hour = 3_600_000;

function demoAccounts(now: number): AccountViewModel[] {
  return [
    {
      id: "demo-primary",
      providerId: "generic.openai_compatible",
      providerName: "OpenAI 兼容",
      label: "OpenAI 官方",
      status: "healthy",
      freshness: "fresh",
      source: "OFFICIAL_API",
      lastUpdatedAt: now - 4 * minute,
      snapshots: [
        {
          id: "demo-primary-5h",
          metricName: "5 小时用量窗口",
          unit: "request",
          used: 128,
          limit: 800,
          remaining: 672,
          window: { type: "rolling", resetAt: now + 2 * hour + 24 * minute },
          observedAt: now - 4 * minute,
          source: "OFFICIAL_API",
          freshness: "fresh",
          status: "available",
        },
        {
          id: "demo-primary-week",
          metricName: "每周额度",
          unit: "currency",
          currency: "$",
          used: 12.4,
          limit: 120,
          remaining: 107.6,
          window: { type: "fixed", resetAt: now + 3 * 24 * hour },
          observedAt: now - 4 * minute,
          source: "OFFICIAL_API",
          freshness: "fresh",
          status: "available",
        },
      ],
    },
    {
      id: "demo-relay",
      providerId: "generic.openai_compatible",
      providerName: "OpenAI 兼容",
      label: "公司中转站",
      status: "attention",
      freshness: "fresh",
      source: "CUSTOM_ENDPOINT",
      lastUpdatedAt: now - 11 * minute,
      snapshots: [
        {
          id: "demo-relay-day",
          metricName: "每日 Token",
          unit: "token",
          used: 1_820_000,
          limit: 2_500_000,
          remaining: 680_000,
          window: { type: "calendar", resetAt: now + 41 * minute },
          observedAt: now - 11 * minute,
          source: "CUSTOM_ENDPOINT",
          freshness: "fresh",
          status: "available",
        },
      ],
    },
    {
      id: "demo-backup",
      providerId: "generic.openai_compatible",
      providerName: "OpenAI 兼容",
      label: "备用网关",
      status: "error",
      freshness: "expired",
      errorCode: "HTTP 502",
      errorMessage: "上游返回 502，已保留最近一次成功的数据。",
      source: "OFFICIAL_API",
      lastUpdatedAt: now - 9 * hour,
      snapshots: [
        {
          id: "demo-backup-5h",
          metricName: "5 小时用量窗口",
          unit: "request",
          used: 310,
          limit: 600,
          remaining: 290,
          window: { type: "rolling", resetAt: now + 55 * minute },
          observedAt: now - 9 * hour,
          source: "CACHED",
          freshness: "expired",
          status: "available",
        },
      ],
    },
  ];
}

export function createMockBridge(): DashboardBridge {
  let accounts = demoAccounts(Date.now());
  const latency = () => 180 + Math.random() * 260;

  return {
    async listAccounts() {
      await new Promise((resolve) => setTimeout(resolve, latency()));
      return structuredClone(accounts);
    },
    async createAccount(input: GenericOpenAiAccountInput) {
      await new Promise((resolve) => setTimeout(resolve, latency()));
      const account: AccountViewModel = {
        id: `demo-${Date.now()}`,
        providerId: input.providerId,
        providerName: "OpenAI 兼容",
        label: input.label,
        status: "healthy",
        freshness: "unknown",
        source: "OFFICIAL_API",
        lastUpdatedAt: Date.now(),
        snapshots: [],
      };
      accounts = [...accounts, account];
      return structuredClone(account);
    },
    async deleteAccount(accountId) {
      await new Promise((resolve) => setTimeout(resolve, latency()));
      accounts = accounts.filter((account) => account.id !== accountId);
    },
    async refresh(accountId) {
      await new Promise((resolve) => setTimeout(resolve, latency() + 220));
      accounts = demoAccounts(Date.now()).map((account, index) =>
        index === 2 && accountId !== "demo-backup"
          ? accounts.find((item) => item.id === account.id) ?? account
          : account,
      );
      if (accountId) {
        return structuredClone(accounts.filter((account) => account.id === accountId));
      }
      return structuredClone(accounts);
    },
    async checkForUpdate() {
      await new Promise((resolve) => setTimeout(resolve, latency()));
      return null;
    },
    async installUpdate() {
      /* demo no-op */
    },
  };
}
