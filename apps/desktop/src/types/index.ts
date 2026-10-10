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
  unit: QuotaUnit | string;
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
  providerId: "generic.openai_compatible";
  label: string;
  baseUrl: string;
  apiKey: string;
  quotaEndpoint?: string;
}

export interface UpdateInfo {
  version: string;
  notes?: string | null;
  publishedAt?: string | null;
}

export interface DashboardBridge {
  listAccounts(): Promise<AccountViewModel[]>;
  createAccount(input: GenericOpenAiAccountInput): Promise<AccountViewModel>;
  deleteAccount(accountId: string): Promise<void>;
  refresh(accountId?: string): Promise<AccountViewModel[]>;
  checkForUpdate(): Promise<UpdateInfo | null>;
  installUpdate(): Promise<void>;
}
