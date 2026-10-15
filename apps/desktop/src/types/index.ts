export type QuotaUnit =
  | "token"
  | "currency"
  | "request"
  | "credit"
  | "point"
  | "compute_unit"
  | "character"
  | "minute"
  | "image"
  | "percent"
  | "unknown";

export type AccountHealth =
  | "healthy"
  | "attention"
  | "critical"
  | "stale"
  | "error"
  | "offline"
  | "syncing"
  | "unavailable";

export type Freshness = "fresh" | "stale" | "expired" | "unknown";

export interface QuotaWindowViewModel {
  type?: string | null;
  startsAt?: string | number | null;
  endsAt?: string | number | null;
  resetAt?: string | number | null;
}

export interface QuotaViewModel {
  id: string;
  metricName: string;
  unit: QuotaUnit;
  currency?: string | null;
  used?: number | null;
  limit?: number | null;
  remaining?: number | null;
  window?: QuotaWindowViewModel | null;
  observedAt?: string | number | null;
  source?: string | null;
  freshness?: Freshness;
  status?: "available" | "partial" | "unavailable" | "error";
}

export interface AccountViewModel {
  id: string;
  providerId: string;
  providerName: string;
  label: string;
  accountType?: string | null;
  status: AccountHealth;
  freshness: Freshness;
  snapshots: QuotaViewModel[];
  source?: string | null;
  lastUpdatedAt?: string | number | null;
  errorCode?: string | null;
  errorMessage?: string | null;
}

export interface GenericOpenAiAccountInput {
  /** "generic.openai_compatible" or a built-in provider id ("deepseek", …). */
  providerId: string;
  label: string;
  baseUrl: string;
  apiKey: string;
  quotaEndpoint?: string;
  usageEndpoint?: string;
  balanceEndpoint?: string;
}

export interface UpdateInfo {
  version: string;
  notes?: string | null;
  publishedAt?: string | null;
}

export type NotificationSeverity = "INFO" | "WARNING" | "CRITICAL";

export type NotificationKind =
  | "low_quota"
  | "reset_approaching"
  | "auth_expired"
  | "provider_unreachable"
  | "balance_below";

export interface NotificationViewModel {
  id: string;
  kind: NotificationKind;
  severity: NotificationSeverity;
  title: string;
  message: string;
  accountId?: string | null;
  providerId?: string | null;
  createdAt: number;
  readAt?: number | null;
}

export interface NotificationPreferencesViewModel {
  enabled: boolean;
  silentMode: boolean;
  criticalOnly: boolean;
  lowQuotaThreshold: number;
  balanceThreshold?: number | null;
  resetSoonWindow: number;
}

export interface HistoryDayViewModel {
  /** UTC timestamp of local midnight, in seconds. */
  dayStart: number;
  tokens: number;
  cost?: number | null;
  currency?: string | null;
  /** False when nothing was observed that day: unknown, not zero. */
  hasData: boolean;
}

export interface HistorySummaryViewModel {
  days: HistoryDayViewModel[];
  totalTokens: number;
  totalCost?: number | null;
  currency?: string | null;
  activeDays: number;
  rangeDays: number;
}

export interface RangeUsageViewModel {
  tokens: number;
  cost?: number | null;
  currency?: string | null;
  /** False when nothing was observed inside the window: unknown, not zero. */
  hasData: boolean;
}

/** Consumption velocity plus how long the remaining quota lasts at it. */
export interface BurnRateViewModel {
  /** Token units consumed per hour over the trailing window. */
  tokensPerHour: number;
  /** Hours until remaining hits zero at that pace, when known. */
  projectedExhaustionHours?: number;
}

export interface UsageWindowsViewModel {
  hours2: RangeUsageViewModel;
  hours24: RangeUsageViewModel;
  days30: RangeUsageViewModel;
  burnRate?: BurnRateViewModel;
}

export interface HistoryDayAccountViewModel {
  accountId: string;
  label: string;
  tokens: number;
  cost?: number | null;
  currency?: string | null;
}

export interface HistoryDayBreakdownViewModel {
  dayStart: number;
  accounts: HistoryDayAccountViewModel[];
}

/** Setup profiles plus the community packs that failed validation. */
export interface ManifestProfilesViewModel {
  profiles: ManifestProfileViewModel[];
  /** "file: reason" entries for skipped community packs. */
  skipped: string[];
}

export interface ProviderMatrixRowViewModel {
  id: string;
  name: string;
  apiFormat: string;
  level?: string | null;
  accountTypes: string[];
  quotaUnits: string[];
  targetWindows: string[];
  priority: string;
  implemented: boolean;
}

export interface ManifestEndpointsViewModel {
  models?: string | null;
  balance?: string | null;
  usage?: string | null;
  quota?: string | null;
  reset?: string | null;
}

export interface ManifestProfileViewModel {
  id: string;
  name: string;
  baseUrl: string;
  apiFormat: string;
  /** Gateways have no fixed base URL — the user supplies one. */
  customBaseUrl: boolean;
  level: string;
  accountTypes: string[];
  quotaUnits: string[];
  targetWindows: string[];
  endpoints: ManifestEndpointsViewModel;
}

export interface DashboardBridge {
  listAccounts(): Promise<AccountViewModel[]>;
  createAccount(input: GenericOpenAiAccountInput): Promise<AccountViewModel>;
  deleteAccount(accountId: string): Promise<void>;
  refresh(accountId?: string): Promise<AccountViewModel[]>;
  checkForUpdate(): Promise<UpdateInfo | null>;
  installUpdate(): Promise<void>;
  listNotifications(limit?: number): Promise<NotificationViewModel[]>;
  markNotificationsRead(ids?: string[]): Promise<void>;
  getNotificationPreferences(): Promise<NotificationPreferencesViewModel>;
  setNotificationPreferences(
    prefs: NotificationPreferencesViewModel,
  ): Promise<void>;
  historySummary(days?: number, accountId?: string): Promise<HistorySummaryViewModel>;
  historyBreakdown(days?: number): Promise<HistoryDayBreakdownViewModel[]>;
  providerMatrix(): Promise<ProviderMatrixRowViewModel[]>;
  manifestProfiles(): Promise<ManifestProfilesViewModel>;
  usageWindows(accountId?: string): Promise<UsageWindowsViewModel>;
}
