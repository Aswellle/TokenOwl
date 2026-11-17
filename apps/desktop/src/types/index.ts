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
  | "unavailable"
  | "unconfigured";

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

/** Structured, secret-free view of the last refresh attempt. Only stable
 * codes, HTTP statuses, endpoint kinds, and timestamps cross the IPC
 * boundary. */
export interface RefreshDiagnostics {
  status: "idle" | "refreshing" | "healthy" | "failed";
  errorCode?: string | null;
  httpStatus?: number | null;
  endpointKind?: "models" | "quota" | "usage" | "balance" | "reset" | null;
  lastAttemptAt?: number | null;
  lastSuccessAt?: number | null;
  nextRetryAt?: number | null;
  /** Next planned background refresh; failure backoff wins when present. */
  nextScheduledAt?: number | null;
  consecutiveFailures?: number;
}

/** Refresh cadence preferences. `cadenceMinutes: 0` means the provider
 * manifests' own defaults; adaptive mode stays inside the configured bounds. */
export interface RefreshSettings {
  adaptive: boolean;
  cadenceMinutes: number;
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
  diagnostics?: RefreshDiagnostics | null;
  /** Provider manifest capability level (L1..L4); null for custom accounts.
   * An always-empty L1/L2 account is "no key-level quota published", not a
   * fetch failure. */
  capabilityLevel?: string | null;
}

/** A known local coding-agent usage source and its privacy contract. */
export interface LocalUsageSourceInfo {
  id: string;
  name: string;
  directories: string[];
  fileTypes: string[];
  collected: string[];
  neverCollected: string[];
  verified: boolean;
}

export interface LocalUsageSourceView {
  sources: LocalUsageSourceInfo[];
  enabled: string[];
}

export interface LocalUsageScanResult {
  sourceId: string;
  scannedFiles: number;
  newEvents: number;
  skipped: string[];
  errors: string[];
}

export interface LocalUsageDayView {
  dayStartUnix: number;
  model: string;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
  eventCount: number;
}

export interface LocalUsageSummary {
  days: LocalUsageDayView[];
  totals: LocalUsageDayView;
  sourceCounts: Array<[string, number]>;
}

/** One endpoint's diagnostic result; no URL, header, or body is included. */
export interface EndpointProbeViewModel {
  kind: "models" | "quota" | "usage" | "balance" | "reset" | string;
  outcome:
    | "ok"
    | "auth"
    | "rate_limited"
    | "timeout"
    | "network"
    | "tls"
    | "server_error"
    | "client_error"
    | "malformed"
    | "config_error"
    | "unsupported"
    | string;
  httpStatus?: number | null;
  errorCode?: string | null;
  retryAt?: string | null;
  latencyMs: number;
}

export interface EndpointTestView {
  accountId: string;
  providerId: string;
  baseHost?: string | null;
  customEndpoint: boolean;
  testedAt: number;
  probes: EndpointProbeViewModel[];
}

export interface DiagnosticsPreview {
  token: string;
  generatedAt: number;
  json: string;
}

export interface DiagnosticsExportResult {
  path: string;
  bytes: number;
}

/** Result of a secret-free account-config export. */
export interface AccountConfigExportResult {
  path: string;
  accounts: number;
}

export interface AccountImportEntryViewModel {
  index: number;
  providerId: string;
  providerName: string;
  label: string;
  baseUrl: string;
  apiFormat: string;
  quotaEndpoint?: string | null;
  usageEndpoint?: string | null;
  balanceEndpoint?: string | null;
  importable: boolean;
  reason?: string | null;
  alreadyExists: boolean;
}

export interface AccountImportPreview {
  schemaVersion: number;
  exportedAt: number;
  entries: AccountImportEntryViewModel[];
}

/** Result of a history export: the file path under the app data dir. */
export interface HistoryExportResult {
  path: string;
  rows: number;
}

/** Result of a credential update. Only `validated_and_saved` means the
 * provider accepted the new key and it replaced the old one; every other
 * outcome leaves the previous key untouched. */
export interface CredentialUpdateResult {
  outcome: "validated_and_saved" | "rejected" | "validation_indeterminate" | "storage_failed";
  errorCode?: string | null;
  retryAt?: string | null;
  account?: AccountViewModel | null;
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
  /** One-time LAN confirmation token from `lanConsentBegin`; Rust decides
   * whether it is required, the frontend never assumes. */
  lanConsentToken?: string;
  /** Wire format of a CUSTOM provider target; built-in ids ignore this. */
  apiFormat?: "openai_compatible" | "anthropic" | "gemini";
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
  | "balance_below"
  | "window_low"
  | "projected_exhaustion"
  | "recovered";

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
  /** Seconds before projected exhaustion still worth warning about; 0 disables. */
  projectedExhaustionHorizon: number;
  /** Quiet hours as minutes since midnight; equal values disable them. */
  quietHoursStart: number;
  quietHoursEnd: number;
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
export interface CommunityPackViewModel {
  id: string;
  name: string;
  file?: string | null;
  schemaVersion: number;
  authMethods: string[];
  accountCount: number;
  lastAttemptAt?: number | null;
  lastStatus?: "idle" | "refreshing" | "healthy" | "failed" | null;
  lastErrorCode?: string | null;
}

export interface ManifestProfilesViewModel {
  profiles: ManifestProfileViewModel[];
  /** "file: reason" entries for skipped community packs. */
  skipped: string[];
  /** Validated community packs and their last request state. */
  community: CommunityPackViewModel[];
}

export interface ProviderMatrixRowViewModel {
  id: string;
  name: string;
  apiFormat: string;
  authMethods: string[];
  schemaVersion: number;
  lastVerifiedAt?: string | null;
  contractVerified: boolean;
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
  /** Platform variants of one brand share this key and one picker capsule. */
  brand?: string;
  /** The platform label inside a brand's variant list (国内版, OpenAI 兼容). */
  variant?: string;
  /** `builtin` or `community`. */
  source: "builtin" | "community";
  /** Community pack file name inside `community-providers/`. */
  sourceFile?: string | null;
  schemaVersion: number;
  authMethods: string[];
  lastVerifiedAt?: string | null;
  level: string;
  accountTypes: string[];
  quotaUnits: string[];
  targetWindows: string[];
  endpoints: ManifestEndpointsViewModel;
}

/**
 * How one request leaves the machine. `system` follows the operating
 * system's proxy configuration; `custom` uses the stored proxy URL.
 */
export type ProxyRoute = "direct" | "system" | "custom";

/** Proxy policy split by provider region. */
export interface ProxyRouting {
  domestic: ProxyRoute;
  international: ProxyRoute;
}

export interface DashboardBridge {
  listAccounts(): Promise<AccountViewModel[]>;
  createAccount(input: GenericOpenAiAccountInput): Promise<AccountViewModel>;
  /** Whether this base URL would talk to the LAN (blocked by default, docs/06). */
  accountTargetIsLan(baseUrl: string): Promise<boolean>;
  /** First half of the LAN consent flow: returns `null` when the target is
   * not LAN, or a short-lived one-time token the confirm dialog presents
   * back through `createAccount`. Rust re-validates on creation. */
  lanConsentBegin(baseUrl: string): Promise<string | null>;
  /** LAN targets the user explicitly trusts; creations against them skip
   * the confirmation prompt. Revocable. */
  lanTrustList(): Promise<string[]>;
  lanTrustAdd(url: string): Promise<void>;
  lanTrustRemove(url: string): Promise<void>;
  deleteAccount(accountId: string): Promise<void>;
  /** Replaces an account's API key in place (history preserved) and returns
   * the account after the validating refresh. */
  updateAccountCredential(accountId: string, apiKey: string): Promise<CredentialUpdateResult>;
  localUsageSources(): Promise<LocalUsageSourceView>;
  setLocalUsageSource(sourceId: string, enabled: boolean): Promise<void>;
  scanLocalUsage(): Promise<LocalUsageScanResult[]>;
  localUsageSummary(days?: number): Promise<LocalUsageSummary>;
  clearLocalUsage(): Promise<number>;
  exportHistory(format: "csv" | "json"): Promise<HistoryExportResult>;
  /** Writes a secret-free backup of the selected accounts. */
  exportAccountConfig(accountIds?: string[]): Promise<AccountConfigExportResult>;
  /** Validates a pasted backup without creating anything. */
  previewAccountImport(payload: string): Promise<AccountImportPreview>;
  /** Runs the per-endpoint diagnostics for one account. */
  testAccountEndpoints(accountId: string): Promise<EndpointTestView>;
  /** Builds the redacted local diagnostic bundle for review. */
  diagnosticsPreview(): Promise<DiagnosticsPreview>;
  /** Writes the exact previewed bundle; rejects when its token is stale. */
  exportDiagnostics(expectedToken: string): Promise<DiagnosticsExportResult>;
  refresh(accountId?: string): Promise<AccountViewModel[]>;
  /** Cadence preference plus the opt-in adaptive policy. */
  getRefreshSettings(): Promise<RefreshSettings>;
  setRefreshSettings(settings: RefreshSettings): Promise<void>;
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
  /** Validates a pasted manifest and runs its mappings on a local sample. */
  usageWindows(accountId?: string): Promise<UsageWindowsViewModel>;
  /** The configured proxy URL, or null for a direct connection. */
  getProxy(): Promise<string | null>;
  /** Stores (or, with null, clears) the proxy URL; applies immediately. */
  setProxy(url: string | null): Promise<void>;
  /** Per-region proxy routing (domestic vs. international providers). */
  getProxyRouting(): Promise<ProxyRouting>;
  /** Persists and applies the routing policy immediately. */
  setProxyRouting(routing: ProxyRouting): Promise<ProxyRouting>;
  /** Whether the user last collapsed the dashboard's bottom dock. */
  getDockCollapsed(): Promise<boolean>;
  /** Persists the dock's collapsed state for the next launch. */
  setDockCollapsed(collapsed: boolean): Promise<void>;
}
