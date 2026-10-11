import { copy } from "../styles/copy";
import type { AccountHealth, AccountViewModel, QuotaViewModel } from "../types";

/* ---------- time & value formatting ---------- */

export function timestamp(value: string | number | null | undefined): number | null {
  if (value === null || value === undefined || value === "") return null;
  const parsed = typeof value === "number" ? value : Date.parse(value);
  if (!Number.isFinite(parsed)) return null;
  return typeof value === "number" && parsed < 1_000_000_000_000 ? parsed * 1000 : parsed;
}

export function formatValue(value: number | null | undefined, quota: QuotaViewModel): string {
  if (value === null || value === undefined || !Number.isFinite(value)) return copy.quota.none;
  const number = new Intl.NumberFormat("zh-CN", { maximumFractionDigits: 2 }).format(value);
  return quota.unit === "currency" && quota.currency ? `${quota.currency} ${number}` : number;
}

export function formatRelative(value: string | number | null | undefined, now: number): string | null {
  const observed = timestamp(value);
  if (observed === null) return null;
  const seconds = Math.max(0, Math.floor((now - observed) / 1000));
  if (seconds < 45) return copy.time.justNow;
  if (seconds < 3600) return copy.time.minutesAgo(Math.floor(seconds / 60));
  if (seconds < 86400) return copy.time.hoursAgo(Math.floor(seconds / 3600));
  return copy.time.daysAgo(Math.floor(seconds / 86400));
}

export function formatCountdown(value: string | number | null | undefined, now: number): string | null {
  const reset = timestamp(value);
  if (reset === null) return null;
  const seconds = Math.max(0, Math.floor((reset - now) / 1000));
  if (seconds === 0) return copy.time.resetDue;
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  if (days > 0) return copy.time.resetInDays(days, hours);
  if (hours > 0) return copy.time.resetInHours(hours, minutes);
  return copy.time.resetInMinutes(Math.max(1, minutes));
}

export function formatResetShort(value: string | number | null | undefined, now: number): string | null {
  const reset = timestamp(value);
  if (reset === null) return null;
  const seconds = Math.max(0, Math.floor((reset - now) / 1000));
  if (seconds === 0) return copy.time.resetDue;
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  if (days > 0) return copy.time.resetShortDays(days, hours);
  if (hours > 0) return copy.time.resetShortHours(hours, minutes);
  return copy.time.resetShortMinutes(Math.max(1, minutes));
}

export function displaySource(source: string | null | undefined): string | null {
  if (!source) return null;
  return copy.sources[source] ?? source;
}

/* ---------- derived view state ---------- */

export function accountResetAt(account: AccountViewModel): string | number | null {
  const reset = account.snapshots
    .map((quota) => quota.window?.resetAt ?? quota.window?.endsAt ?? null)
    .map((value) => ({ value, time: timestamp(value) }))
    .filter((item): item is { value: string | number; time: number } => item.time !== null)
    .sort((left, right) => left.time - right.time)[0];
  return reset?.value ?? null;
}

export function overallHealth(accounts: AccountViewModel[], phase: "loading" | "ready" | "offline"): AccountHealth {
  if (phase === "loading" && accounts.length === 0) return "syncing";
  if (phase === "offline" && accounts.length === 0) return "offline";
  if (accounts.some((account) => account.status === "error")) return "error";
  if (accounts.some((account) => account.status === "critical")) return "critical";
  if (accounts.some((account) => account.status === "attention")) return "attention";
  if (accounts.some((account) => account.status === "stale")) return "stale";
  if (accounts.some((account) => account.status === "syncing")) return "syncing";
  if (accounts.some((account) => account.status === "unavailable" || account.status === "offline")) {
    return "unavailable";
  }
  return "healthy";
}

/** Fraction of quota still available, or null when the provider reports neither pair. */
export function quotaRatio(quota: QuotaViewModel): number | null {
  const clamp = (value: number) => Math.min(1, Math.max(0, value));
  const { remaining, limit, used } = quota;
  if (limit !== null && limit !== undefined && limit > 0) {
    if (remaining !== null && remaining !== undefined) return clamp(remaining / limit);
    if (used !== null && used !== undefined) return clamp(1 - used / limit);
  }
  return null;
}

export function pressure(snapshots: QuotaViewModel[]): number | null {
  const ratios = snapshots.map(quotaRatio).filter((value): value is number => value !== null);
  return ratios.length > 0 ? Math.min(...ratios) : null;
}

// PRD health thresholds, identical to the Rust domain grading (domain/health.rs):
// strictly below 10% is critical, strictly below 25% is attention.
const CRITICAL_THRESHOLD = 0.1;
const ATTENTION_THRESHOLD = 0.25;

/** Threshold tone shared by bars, rings and chips: <10% critical, <25% attention. */
export function ratioTone(ratio: number | null): "healthy" | "attention" | "critical" | null {
  if (ratio === null) return null;
  return ratio < CRITICAL_THRESHOLD
    ? "critical"
    : ratio < ATTENTION_THRESHOLD
      ? "attention"
      : "healthy";
}
