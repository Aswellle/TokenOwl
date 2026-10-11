import type {
  AccountViewModel,
  DashboardBridge,
  GenericOpenAiAccountInput,
  HistoryDayViewModel,
  HistorySummaryViewModel,
  NotificationPreferencesViewModel,
  ManifestProfileViewModel,
  NotificationViewModel,
  ProviderMatrixRowViewModel,
  UsageWindowsViewModel,
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
      providerId: "openai:demo-primary",
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
      providerId: "anthropic:demo-relay",
      providerName: "Anthropic",
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
      providerId: "deepseek:demo-multi",
      providerName: "DeepSeek",
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
      providerId: "gemini:demo-backup",
      providerName: "Google Gemini",
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
      providerId: "anthropic:demo-relay",
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
      providerId: "anthropic:demo-relay",
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
      providerId: "gemini:demo-backup",
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

/** Deterministic PRNG so the demo heatmap is stable within a session. */
function mulberry32(seed: number): () => number {
  return () => {
    seed = (seed + 0x6d2b79f5) | 0;
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function demoHistory(now: number): HistorySummaryViewModel {
  const rand = mulberry32(20261011);
  const rangeDays = 182;
  const recordingDays = 120;
  const today = new Date(new Date(now).toDateString());
  const days: HistoryDayViewModel[] = [];
  let totalTokens = 0;
  let totalCost = 0;
  let activeDays = 0;
  for (let offset = rangeDays - 1; offset >= 0; offset--) {
    const date = new Date(today.getTime() - offset * 86_400_000);
    const dayStart = Math.floor(date.getTime() / 1000);
    const hasData = offset < recordingDays;
    const weekday = date.getDay();
    const weekendFactor = weekday === 0 || weekday === 6 ? 0.35 : 1;
    // Adoption ramp: usage grows as the app keeps recording.
    const ramp = offset > recordingDays - 30 ? 1 : offset > recordingDays - 75 ? 0.55 : 0.2;
    const active = hasData && rand() > 0.14 ? 1 : 0;
    const tokens = active
      ? Math.round((150_000 + rand() * 1_600_000) * weekendFactor * ramp)
      : 0;
    const cost = active ? Math.round((tokens / 1_000_000) * 4.5 * (0.8 + rand() * 0.4) * 100) / 100 : 0;
    if (tokens > 0) activeDays += 1;
    totalTokens += tokens;
    totalCost += cost;
    days.push({
      dayStart,
      tokens,
      cost: cost > 0 ? cost : null,
      currency: cost > 0 ? "¥" : null,
      hasData,
    });
  }
  return {
    days,
    totalTokens,
    totalCost: totalCost > 0 ? Math.round(totalCost * 100) / 100 : null,
    currency: totalCost > 0 ? "¥" : null,
    activeDays,
    rangeDays,
  };
}

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
    async historySummary(days?: number) {
      await new Promise((resolve) => setTimeout(resolve, latency()));
      const summary = demoHistory(Date.now());
      return structuredClone({ ...summary, rangeDays: days ?? summary.rangeDays });
    },
    async manifestProfiles() {
      await new Promise((resolve) => setTimeout(resolve, latency()));
      const profile = (
        id: string,
        name: string,
        baseUrl: string,
        level: string,
        endpoints: ManifestProfileViewModel["endpoints"],
      ): ManifestProfileViewModel => ({
        id,
        name,
        baseUrl,
        level,
        accountTypes: ["api"],
        quotaUnits: ["token"],
        targetWindows: ["provider-defined"],
        endpoints,
      });
      return [
        profile("openai", "OpenAI", "https://api.openai.com/v1", "L4", {
          models: "/v1/models",
          usage: "/v1/usage",
          quota: "/v1/quota",
        }),
        profile("anthropic", "Anthropic", "https://api.anthropic.com/v1", "L4", {
          models: "/v1/models",
        }),
        profile("deepseek", "DeepSeek", "https://api.deepseek.com/v1", "L3", {
          models: "/models",
          balance: "/user/balance",
        }),
        profile("kimi", "Kimi / Moonshot", "https://api.moonshot.cn/v1", "L3", {
          models: "/models",
        }),
        profile("gemini", "Google Gemini", "https://generativelanguage.googleapis.com/v1beta", "L3", {
          models: "/models",
        }),
        profile("minimax", "MiniMax", "https://api.minimax.chat/v1", "L2", {
          models: "/models",
        }),
      ];
    },
    async providerMatrix() {
      await new Promise((resolve) => setTimeout(resolve, latency()));
      const row = (
        id: string,
        name: string,
        level: string | null,
        accountTypes: string[],
        quotaUnits: string[],
        targetWindows: string[],
        priority: string,
        implemented = true,
      ): ProviderMatrixRowViewModel => ({
        id,
        name,
        level,
        accountTypes,
        quotaUnits,
        targetWindows,
        priority,
        implemented,
      });
      return [
        row("openai", "OpenAI", "L4", ["api", "usage"], ["currency", "token"], ["monthly", "custom"], "P0"),
        row("anthropic", "Anthropic", "L4", ["subscription", "api"], ["token"], ["5h", "7d", "usage"], "P0"),
        row("openrouter", "OpenRouter", "L4", ["prepaid", "api"], ["currency", "token"], ["balance", "usage"], "P0"),
        row("deepseek", "DeepSeek", "L3", ["prepaid", "api"], ["currency", "token"], ["balance", "usage"], "P0"),
        row("qwen", "Qwen / Alibaba", "L3", ["coding", "token", "api"], ["token", "credit"], ["5h", "7d", "balance"], "P0"),
        row("kimi", "Kimi / Moonshot", "L3", ["subscription", "api"], ["token", "balance"], ["5h", "7d", "balance"], "P0"),
        row("gemini", "Google Gemini", "L3", ["subscription", "api"], ["quota", "token"], ["provider-defined"], "P0"),
        row("minimax", "MiniMax", "L2", ["coding", "api"], ["token", "credit"], ["provider-defined"], "P0"),
        row("new-api", "New API", null, ["gateway"], ["custom"], ["custom"], "P0", false),
        row("litellm", "LiteLLM", null, ["gateway"], ["custom"], ["custom"], "P0", false),
        row("zhipu", "Zhipu / GLM", null, ["subscription", "api"], ["quota", "token"], ["provider-defined"], "P1", false),
        row("groq", "Groq", null, ["api"], ["request", "token", "rate"], ["hourly", "provider-defined"], "P1", false),
        row("xai", "xAI", null, ["api"], ["currency", "token"], ["billing"], "P1", false),
        row("iflytek", "iFlytek", null, ["api"], ["quota", "points"], ["provider-defined"], "P2", false),
      ];
    },
    async historyBreakdown(days?: number) {
      await new Promise((resolve) => setTimeout(resolve, latency()));
      const range = days ?? 30;
      const summary = demoHistory(Date.now());
      const rand = mulberry32(42);
      const roster: Array<{ id: string; label: string; weight: number }> = [
        { id: "demo-primary", label: "OpenAI 官方", weight: 0.42 },
        { id: "demo-relay", label: "公司中转站", weight: 0.28 },
        { id: "demo-multi", label: "多端点网关", weight: 0.22 },
        { id: "demo-backup", label: "备用网关", weight: 0.08 },
      ];
      return summary.days
        .filter((day) => day.hasData && day.tokens > 0)
        .slice(-range)
        .map((day) => ({
          dayStart: day.dayStart,
          accounts: roster
            .map((member) => ({
              accountId: member.id,
              label: member.label,
              tokens: Math.round(day.tokens * member.weight * (0.85 + rand() * 0.3)),
              cost:
                day.cost !== null
                  ? Math.round((day.cost ?? 0) * member.weight * 100) / 100
                  : null,
              currency: day.currency,
            }))
            .filter((entry) => entry.tokens > 0),
        }));
    },
    async usageWindows(accountId?: string) {
      await new Promise((resolve) => setTimeout(resolve, latency()));
      void accountId;
      const now = Date.now();
      const rand = mulberry32(Math.floor(now / 3_600_000));
      const window = (tokens: number, cost: number, hasData: boolean) => ({
        tokens,
        cost: hasData && cost > 0 ? cost : null,
        currency: hasData && cost > 0 ? "¥" : null,
        hasData,
      });
      const usage: UsageWindowsViewModel = {
        hours2: window(Math.round(30_000 + rand() * 90_000), Math.round(rand() * 40) / 100, true),
        hours24: window(Math.round(600_000 + rand() * 900_000), Math.round(rand() * 500) / 100, true),
        days30: window(Math.round(18_000_000 + rand() * 8_000_000), 380 + Math.round(rand() * 100) / 100, true),
      };
      return usage;
    },
  };
}
