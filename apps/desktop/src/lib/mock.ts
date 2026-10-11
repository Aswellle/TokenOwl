import type {
  AccountViewModel,
  DashboardBridge,
  GenericOpenAiAccountInput,
  NotificationPreferencesViewModel,
  NotificationViewModel,
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
      id: "demo-multi",
      providerId: "generic.openai_compatible",
      providerName: "OpenAI 兼容",
      label: "多端点网关",
      status: "healthy",
      freshness: "fresh",
      source: "CUSTOM_ENDPOINT",
      lastUpdatedAt: now - 2 * minute,
      snapshots: [
        {
          id: "demo-multi-quota",
          metricName: "quota",
          unit: "token",
          used: 90,
          limit: 4_000,
          remaining: 3_910,
          window: { type: "subscription", resetAt: now + 5 * 24 * hour },
          observedAt: now - 2 * minute,
          source: "CUSTOM_ENDPOINT",
          freshness: "fresh",
          status: "available",
        },
        {
          id: "demo-multi-usage",
          metricName: "usage",
          unit: "request",
          used: 3_216,
          observedAt: now - 2 * minute,
          source: "CUSTOM_ENDPOINT",
          freshness: "fresh",
          status: "partial",
        },
        {
          id: "demo-multi-balance",
          metricName: "balance",
          unit: "currency",
          currency: "¥",
          remaining: 86.4,
          observedAt: now - 2 * minute,
          source: "CUSTOM_ENDPOINT",
          freshness: "fresh",
          status: "partial",
        },
        {
          id: "demo-multi-reset",
          metricName: "quota",
          unit: "unknown",
          observedAt: now - 2 * minute,
          source: "CUSTOM_ENDPOINT",
          freshness: "fresh",
          status: "error",
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

function demoNotifications(now: number): NotificationViewModel[] {
  return [
    {
      id: "demo-ntf-1",
      kind: "low_quota",
      severity: "CRITICAL",
      title: "额度紧张",
      message: "「公司中转站」的每日 Token 仅剩 5%。",
      accountId: "demo-relay",
      providerId: "generic.openai_compatible:demo-relay",
      createdAt: now - 6 * minute,
      readAt: null,
    },
    {
      id: "demo-ntf-2",
      kind: "reset_approaching",
      severity: "INFO",
      title: "额度即将重置",
      message: "「公司中转站」的额度将在约 45 分钟后重置。",
      accountId: "demo-relay",
      providerId: "generic.openai_compatible:demo-relay",
      createdAt: now - 40 * minute,
      readAt: null,
    },
    {
      id: "demo-ntf-3",
      kind: "provider_unreachable",
      severity: "WARNING",
      title: "长时间无法连接",
      message: "「备用网关」已连续 3 次刷新失败，请检查网络或服务状态。",
      accountId: "demo-backup",
      providerId: "generic.openai_compatible:demo-backup",
      createdAt: now - 3 * hour,
      readAt: now - 2 * hour,
    },
  ];
}

const demoPreferences: NotificationPreferencesViewModel = {
  enabled: true,
  silentMode: false,
  criticalOnly: false,
  lowQuotaThreshold: 20,
  balanceThreshold: null,
  resetSoonWindow: 1_800,
};

export function createMockBridge(): DashboardBridge {
  let accounts = demoAccounts(Date.now());
  let notifications = demoNotifications(Date.now());
  let preferences: NotificationPreferencesViewModel = { ...demoPreferences };
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
      const fresh = demoAccounts(Date.now());
      accounts = fresh.map((account, index) => {
        if (index === 2 && accountId !== "demo-backup") {
          return accounts.find((item) => item.id === account.id) ?? account;
        }
        return account;
      });
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
    async listNotifications(limit?: number) {
      await new Promise((resolve) => setTimeout(resolve, latency()));
      return structuredClone(notifications.slice(0, limit ?? 50));
    },
    async markNotificationsRead(ids?: string[]) {
      await new Promise((resolve) => setTimeout(resolve, latency()));
      const now = Date.now();
      notifications = notifications.map((notification) =>
        ids && ids.length > 0
          ? ids.includes(notification.id) && notification.readAt === null
            ? { ...notification, readAt: now }
            : notification
          : { ...notification, readAt: notification.readAt ?? now },
      );
    },
    async getNotificationPreferences() {
      await new Promise((resolve) => setTimeout(resolve, latency()));
      return structuredClone(preferences);
    },
    async setNotificationPreferences(prefs: NotificationPreferencesViewModel) {
      await new Promise((resolve) => setTimeout(resolve, latency()));
      preferences = { ...prefs };
    },
  };
}
