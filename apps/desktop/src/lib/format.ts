import { copy } from "../styles/copy";
import type { AccountHealth, AccountViewModel, BurnRateViewModel, QuotaViewModel } from "../types";

/* ---------- time & value formatting ---------- */

export function timestamp(value: string | number | null | undefined): number | null {
  if (value === null || value === undefined || value === "") return null;
  const parsed = typeof value === "number" ? value : Date.parse(value);
  if (!Number.isFinite(parsed)) return null;
  return typeof value === "number" && parsed < 1_000_000_000_000 ? parsed * 1000 : parsed;
}

export function formatValue(value: number | null | undefined, quota: QuotaViewModel): string {
  if (value === null || value === undefined || !Number.isFinite(value)) return copy.quota.none;
  // Percent quotas read as a share, not a count: "82%" not "82".
  if (quota.unit === "percent") {
    return `${new Intl.NumberFormat("zh-CN", { maximumFractionDigits: 1 }).format(value)}%`;
  }
  // Token counts get the 万/亿 scale; a raw 1,820,000 forces counting zeros.
  if (quota.unit === "token" && Math.abs(value) >= 10_000) {
    return formatCompact(value);
  }
  const number = new Intl.NumberFormat("zh-CN", { maximumFractionDigits: 2 }).format(value);
  return quota.unit === "currency" && quota.currency ? `${quota.currency} ${number}` : number;
}

/** Compact magnitude for large counts, using the Chinese 万/亿 scale. */
export function formatCompact(value: number | null | undefined): string {
  if (value === null || value === undefined || !Number.isFinite(value)) return copy.summary.none;
  const trim = (scaled: number) =>
    new Intl.NumberFormat("zh-CN", { maximumFractionDigits: 2 }).format(scaled);
  const abs = Math.abs(value);
  if (abs >= 1e8) return `${trim(value / 1e8)} 亿`;
  if (abs >= 1e4) return `${trim(value / 1e4)} 万`;
  return new Intl.NumberFormat("zh-CN", {
    maximumFractionDigits: abs < 10 ? 2 : 0,
  }).format(value);
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
  // A freshly created account carries no snapshots yet; treat a missing array
  // as empty instead of crashing the summary chain that reads it.
  const reset = (account.snapshots ?? [])
    .map((quota) => quota.window?.resetAt ?? quota.window?.endsAt ?? null)
    .map((value) => ({ value, time: timestamp(value) }))
    .filter((item): item is { value: string | number; time: number } => item.time !== null)
    .sort((left, right) => left.time - right.time)[0];
  return reset?.value ?? null;
}

export function overallHealth(accounts: AccountViewModel[], phase: "loading" | "ready" | "offline"): AccountHealth {
  if (phase === "loading" && accounts.length === 0) return "syncing";
  if (phase === "offline" && accounts.length === 0) return "offline";
  // Zero accounts on a ready shell is not "healthy": nothing is being
  // monitored yet, and a green check would overstate the app's state.
  if (accounts.length === 0) return "unconfigured";
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
  const ratios = (snapshots ?? [])
    .map(quotaRatio)
    .filter((value): value is number => value !== null);
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

/**
 * Short window tag from the quota window's own span: the PRD's canonical
 * "5h"/"7d" when the span matches, otherwise the actual span in h/d.
 */
export function quotaWindowTag(quota: QuotaViewModel): string | null {
  const start = quota.window?.startsAt ? Date.parse(String(quota.window.startsAt)) : NaN;
  const end = quota.window?.endsAt ? Date.parse(String(quota.window.endsAt)) : NaN;
  if (Number.isFinite(start) && Number.isFinite(end) && end > start) {
    const hours = (end - start) / 3_600_000;
    if (hours >= 4 && hours <= 6) return "5h";
    if (hours >= 144 && hours <= 192) return "7d";
    return hours >= 24 ? `${Math.round(hours / 24)}d` : `${Math.round(hours)}h`;
  }
  // No span reported: fall back to the provider's own metric naming so the
  // tag reflects what the provider calls the window instead of "unknown".
  const text = `${quota.metricName ?? ""} ${quota.id ?? ""}`;
  if (/5\s*小时|5\s*h/i.test(text)) return "5h";
  if (/7\s*天|7\s*d|周/i.test(text)) return "7d";
  const hoursMatch = /(\d+)\s*小时/.exec(text);
  if (hoursMatch) return `${hoursMatch[1]}h`;
  const daysMatch = /(\d+)\s*天/.exec(text);
  if (daysMatch) return `${daysMatch[1]}d`;
  return null;
}

/** Formats the burn-rate card: "1.2 万/h" plus nothing else — the exhaustion
 * horizon lives in the hint line so the number stays scannable. */
export function formatBurnRate(rate: BurnRateViewModel): string {
  return `${formatCompact(rate.tokensPerHour)}/h`;
}

/** Human hint for the exhaustion projection; `null` when unknown. */
export function burnExhaustionHint(rate: BurnRateViewModel): string | null {
  if (rate.projectedExhaustionHours === undefined) return null;
  const hours = rate.projectedExhaustionHours;
  if (hours < 1) return `按当前速率，剩余额度约 ${Math.round(hours * 60)} 分钟后耗尽`;
  if (hours < 48) return `按当前速率，剩余额度约 ${Math.round(hours)} 小时后耗尽`;
  return `按当前速率，剩余额度约 ${(hours / 24).toFixed(1)} 天后耗尽`;
}
