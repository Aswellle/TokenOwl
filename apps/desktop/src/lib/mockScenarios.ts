import type {
  AccountViewModel,
  HistorySummaryViewModel,
  NotificationViewModel,
  QuotaViewModel,
} from "../types";

/**
 * Dev-only state fixtures for the plain-browser preview.
 *
 * The e2e suite has to prove what the UI does in states a healthy demo account
 * never reaches — offline, stale, unknown, full, three real windows, failed
 * mutations. The Tauri backend cannot be driven from Playwright, so the mock
 * bridge reads a comma-separated `?scenario=` list and derives those states
 * here. Nothing in this file is imported by a production build (`mock.ts` is
 * already gated behind `import.meta.env.DEV`).
 */
const params =
  typeof window === "undefined" ? new URLSearchParams() : new URLSearchParams(window.location.search);

const flags = new Set(
  (params.get("scenario") ?? "")
    .split(",")
    .map((flag) => flag.trim())
    .filter(Boolean),
);

export function hasScenario(name: string): boolean {
  return flags.has(name);
}

export function scenarioList(): string[] {
  return Array.from(flags);
}

function quota(
  id: string,
  metricName: string,
  values: Pick<QuotaViewModel, "used" | "limit" | "remaining">,
  window: QuotaViewModel["window"],
  extra: Partial<QuotaViewModel> = {},
): QuotaViewModel {
  return {
    id,
    metricName,
    unit: "request",
    ...values,
    window,
    observedAt: Date.now(),
    source: "OFFICIAL_API",
    freshness: "fresh",
    status: "available",
    ...extra,
  };
}

/** Three distinct, real windows: 5h / 7d / 30d. */
function threeWindowSnapshots(now: number): QuotaViewModel[] {
  return [
    quota(
      "scenario-5h",
      "5 小时用量窗口",
      { used: 320, limit: 800, remaining: 480 },
      {
        type: "rolling",
        startsAt: new Date(now - 2 * 3_600_000).toISOString(),
        endsAt: new Date(now + 3 * 3_600_000).toISOString(),
        resetAt: now + 3 * 3_600_000,
      },
    ),
    quota(
      "scenario-7d",
      "每周额度",
      { used: 70, limit: 100, remaining: 30 },
      {
        type: "fixed",
        startsAt: new Date(now - 2 * 86_400_000).toISOString(),
        endsAt: new Date(now + 5 * 86_400_000).toISOString(),
        resetAt: now + 5 * 86_400_000,
      },
    ),
    quota(
      "scenario-30d",
      "每月额度",
      { used: 100, limit: 1_000, remaining: 900 },
      {
        type: "fixed",
        startsAt: new Date(now - 10 * 86_400_000).toISOString(),
        endsAt: new Date(now + 20 * 86_400_000).toISOString(),
        resetAt: now + 20 * 86_400_000,
      },
    ),
  ];
}

function fullSnapshots(now: number): QuotaViewModel[] {
  return [
    quota(
      "scenario-full-5h",
      "5 小时用量窗口",
      { used: 800, limit: 800, remaining: 0 },
      { type: "rolling", resetAt: now + 3_600_000 },
    ),
    quota(
      "scenario-full-7d",
      "每周额度",
      { used: 120, limit: 120, remaining: 0 },
      { type: "fixed", resetAt: now + 4 * 86_400_000 },
    ),
  ];
}

/**
 * One account carrying every quota KIND the UI must tell apart: a 5h rolling
 * token window, a 7d fixed window, prepaid credits with a cap, a currency
 * balance without any window, named balance splits, and a percent reading.
 */
function mixedKindSnapshots(now: number): QuotaViewModel[] {
  return [
    quota(
      "scenario-kind-5h",
      "5 小时用量窗口",
      { used: 500_000, limit: 1_000_000, remaining: 500_000 },
      {
        type: "rolling",
        startsAt: new Date(now - 3 * 3_600_000).toISOString(),
        endsAt: new Date(now + 2 * 3_600_000).toISOString(),
        resetAt: now + 2 * 3_600_000,
      },
      { unit: "token" },
    ),
    quota(
      "scenario-kind-7d",
      "每周额度",
      { used: 8_200_000, limit: 10_000_000, remaining: 1_800_000 },
      {
        type: "fixed",
        startsAt: new Date(now - 5 * 86_400_000).toISOString(),
        endsAt: new Date(now + 2 * 86_400_000).toISOString(),
        resetAt: now + 2 * 86_400_000,
      },
      { unit: "token" },
    ),
    quota(
      "scenario-kind-credits",
      "积分",
      { used: 120, limit: 200, remaining: 80 },
      { type: "subscription", resetAt: now + 12 * 86_400_000 },
      { unit: "credit" },
    ),
    quota(
      "scenario-kind-cash",
      "现金余额",
      { used: null, limit: null, remaining: 3.0 },
      null,
      { unit: "currency", currency: "CNY" },
    ),
    quota(
      "scenario-kind-grant",
      "赠送余额",
      { used: null, limit: null, remaining: 0.88 },
      null,
      { unit: "currency", currency: "CNY" },
    ),
    quota(
      "scenario-kind-usage",
      "输入 Token 用量",
      { used: 18_234_567, limit: null, remaining: null },
      null,
      { unit: "token" },
    ),
  ];
}

function unknownSnapshots(now: number): QuotaViewModel[] {
  return [
    quota(
      "scenario-unknown-5h",
      "5 小时用量窗口",
      { used: null, limit: null, remaining: null },
      { type: "rolling", resetAt: now + 2 * 3_600_000 },
    ),
    quota(
      "scenario-unknown-7d",
      "每周额度",
      { used: null, limit: null, remaining: null },
      { type: "fixed", resetAt: now + 3 * 86_400_000 },
    ),
  ];
}

function partialSnapshots(now: number): QuotaViewModel[] {
  return [
    quota(
      "scenario-partial-5h",
      "5 小时用量窗口",
      { used: 100, limit: 200, remaining: null },
      { type: "rolling", resetAt: now + 2 * 3_600_000 },
      { status: "partial" },
    ),
    quota(
      "scenario-error-7d",
      "每周额度",
      { used: 10, limit: 100, remaining: 90 },
      { type: "fixed", resetAt: now + 3 * 86_400_000 },
      { status: "error" },
    ),
    {
      id: "scenario-partial-balance",
      metricName: "balance",
      unit: "currency",
      currency: "¥",
      remaining: 42,
      observedAt: now,
      source: "OFFICIAL_API",
      freshness: "fresh",
      status: "partial",
    },
  ];
}

function applyAccountFlags(accounts: AccountViewModel[], now: number): AccountViewModel[] {
  let next = accounts;
  if (hasScenario("many")) {
    const extra: AccountViewModel[] = Array.from({ length: 4 }, (_, index) => {
      const base = accounts[index % accounts.length];
      return {
        ...structuredClone(base),
        id: `scenario-extra-${index}`,
        providerId: `${base.providerId.split(":")[0]}:scenario-extra-${index}`,
        label: `扩展账户 ${index + 1}`,
      };
    });
    next = [...next, ...extra];
  }
  if (hasScenario("long")) {
    next = next.map((account) => ({
      ...account,
      label: `${account.label} · 这是一个非常非常长的账户名称用于验证窄窗口下的截断与换行行为`,
      providerName: `${account.providerName}（企业版 · 华东多可用区网关）`,
    }));
  }
  if (next.length === 0) return next;

  if (hasScenario("stale")) {
    next = next.map((account) => ({
      ...account,
      status: "stale",
      freshness: "stale",
      lastUpdatedAt: now - 3 * 3_600_000,
      snapshots: account.snapshots.map((snapshot) => ({
        ...snapshot,
        freshness: "stale",
        observedAt: now - 3 * 3_600_000,
      })),
    }));
  }
  if (hasScenario("error")) {
    const first = next[0];
    next = [
      {
        ...first,
        status: "error",
        freshness: "expired",
        errorCode: "HTTP 503",
        errorMessage: "上游服务暂时不可用，已保留最近一次成功的数据。",
      },
      ...next.slice(1),
    ];
  }
  if (hasScenario("error-no-snapshots")) {
    next = next.map((account) => ({
      ...account,
      status: "error",
      freshness: "unknown",
      errorCode: "InvalidResponse",
      errorMessage: "response contains no recognized quota fields",
      snapshots: [],
    }));
  }
  if (hasScenario("syncing")) {
    next = next.map((account) => ({ ...account, status: "syncing" }));
  }
  if (hasScenario("no-snapshots")) {
    next = next.map((account) => ({ ...account, snapshots: [], freshness: "unknown" }));
  }
  if (hasScenario("three-windows")) {
    next = [{ ...next[0], snapshots: threeWindowSnapshots(now), status: "healthy" }, ...next.slice(1)];
  }
  if (hasScenario("full")) {
    next = [{ ...next[0], snapshots: fullSnapshots(now), status: "critical" }, ...next.slice(1)];
  }
  if (hasScenario("kinds")) {
    next = [{ ...next[0], snapshots: mixedKindSnapshots(now), status: "healthy" }, ...next.slice(1)];
  }
  if (hasScenario("unknown")) {
    next = [{ ...next[0], snapshots: unknownSnapshots(now) }, ...next.slice(1)];
  }
  if (hasScenario("partial")) {
    next = [{ ...next[0], snapshots: partialSnapshots(now) }, ...next.slice(1)];
  }
  if (hasScenario("balance-only")) {
    next = [
      {
        ...next[0],
        status: "critical",
        snapshots: [
          {
            id: "scenario-balance",
            metricName: "balance",
            unit: "currency",
            currency: "CNY",
            remaining: -0.23,
            observedAt: now,
            source: "OFFICIAL_API",
            freshness: "fresh",
            status: "available",
          },
        ],
      },
      ...next.slice(1).map((account) => ({ ...account, snapshots: [] })),
    ];
  }
  return next;
}

export function scenarioAccounts(accounts: AccountViewModel[], now: number): AccountViewModel[] {
  return applyAccountFlags(accounts, now);
}

export function scenarioHistory(summary: HistorySummaryViewModel): HistorySummaryViewModel {
  if (!hasScenario("history-empty")) return summary;
  return {
    days: [],
    totalTokens: 0,
    totalCost: null,
    currency: null,
    activeDays: 0,
    rangeDays: summary.rangeDays,
  };
}

export function scenarioNotifications(items: NotificationViewModel[]): NotificationViewModel[] {
  if (hasScenario("notifications-empty")) return [];
  if (hasScenario("notifications-read")) {
    return items.map((item) => ({ ...item, readAt: item.readAt ?? Date.now() }));
  }
  return items;
}

export const scenarioLatencyMs = (): number => (hasScenario("slow") ? 2_500 : 0);
