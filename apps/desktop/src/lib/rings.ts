/**
 * Pure rules behind the bubble's activity rings.
 *
 * Everything here is data → data: no DOM, no animation, no React. The bubble
 * view and the SVG component stay thin so the product semantics (which
 * windows are shown, what a ring's ratio means) are testable in isolation.
 *
 * Semantics that must not drift:
 * - `quotaRatio()` is the **remaining** share; the rings draw the **consumed**
 *   share (`1 - remaining`), so a full dial means the quota is used up.
 * - `null` means unknown. It never becomes 0, and an unknown window still
 *   gets a track-only ring instead of a fake empty arc.
 * - At most three rings, only from real, distinct, trusted windowed quotas.
 *   Nothing is averaged, duplicated or invented to fill a ring.
 */
import { pressure, quotaRatio, quotaWindowTag } from "./format";
import type { QuotaViewModel } from "../types";

export type RingTone = "outer" | "middle" | "inner";

export interface RingDatum {
  /** Snapshot id the ring came from. */
  id: string;
  /** Window tag as the provider reports it ("5h", "7d", "30d", …). */
  label: string;
  /** Share of the quota already consumed, 0..1, or null when unknown. */
  consumedRatio: number | null;
  tone: RingTone;
}

export interface RingGeometry {
  tone: RingTone;
  /** Centre-line radius in viewBox units (viewBox is 50×50). */
  radius: number;
  /** Stroke width in viewBox units. */
  width: number;
}

/** Hard ceiling: the reference shows three rings; TokenOwl never invents more. */
export const MAX_BUBBLE_RINGS = 3;

const clampUnit = (value: number): number => Math.min(1, Math.max(0, value));

/**
 * Share of the quota already consumed.
 *
 * Derived from `quotaRatio()` so every surface keeps one remaining-ratio rule;
 * the result is clamped because a provider can report remaining above the
 * limit (bonus credit) or a negative usage delta.
 */
export function quotaConsumedRatio(quota: QuotaViewModel): number | null {
  const remaining = quotaRatio(quota);
  return remaining === null ? null : clampUnit(1 - remaining);
}

/** Window span in hours from the quota's own start/end; 0 when unreported. */
export function quotaSpanHours(quota: QuotaViewModel): number {
  const start = quota.window?.startsAt ? Date.parse(String(quota.window.startsAt)) : NaN;
  const end = quota.window?.endsAt ? Date.parse(String(quota.window.endsAt)) : NaN;
  if (!Number.isFinite(start) || !Number.isFinite(end) || end <= start) return 0;
  return (end - start) / 3_600_000;
}

/**
 * A window is trustworthy when the provider actually returned it (`available`
 * or `partial`) and it carries a window identity. `status` absent means
 * "available" — the same default the quota rows use.
 */
function isTrustedWindowed(quota: QuotaViewModel): boolean {
  const status = quota.status ?? "available";
  if (status !== "available" && status !== "partial") return false;
  return quotaWindowTag(quota) !== null;
}

/**
 * Pick the rings the bubble shows: the shortest and the longest trusted
 * window, plus — only when a third distinct one exists — the window furthest
 * from both ends so the three rings stay semantically separate.
 *
 * Duplicate tags collapse to one ring: two snapshots that call themselves
 * "5h" are the same logical window, and drawing them twice would read as two
 * windows. Provider order breaks ties, so the choice is deterministic.
 */
export function selectBubbleRings(snapshots: QuotaViewModel[]): RingDatum[] {
  const seen = new Set<string>();
  const candidates: Array<{ quota: QuotaViewModel; label: string; span: number }> = [];
  for (const quota of snapshots ?? []) {
    if (!isTrustedWindowed(quota)) continue;
    const label = quotaWindowTag(quota);
    if (label === null || seen.has(label)) continue;
    seen.add(label);
    candidates.push({ quota, label, span: quotaSpanHours(quota) });
  }
  if (candidates.length === 0) return [];

  const sorted = candidates
    .map((entry, index) => ({ ...entry, index }))
    .sort((left, right) => left.span - right.span || left.index - right.index);

  let picked = sorted;
  if (sorted.length > MAX_BUBBLE_RINGS) {
    const shortest = sorted[0];
    const longest = sorted[sorted.length - 1];
    let middle = sorted[1];
    let bestDistance = Number.NEGATIVE_INFINITY;
    for (const entry of sorted.slice(1, -1)) {
      const distance = Math.min(entry.span - shortest.span, longest.span - entry.span);
      if (distance > bestDistance) {
        bestDistance = distance;
        middle = entry;
      }
    }
    picked = [shortest, middle, longest];
  }

  const tones: RingTone[] =
    picked.length === 1
      ? ["outer"]
      : picked.length === 2
        ? ["outer", "inner"]
        : ["outer", "middle", "inner"];

  return picked.map((entry, index) => ({
    id: entry.quota.id,
    label: entry.label,
    consumedRatio: quotaConsumedRatio(entry.quota),
    tone: tones[index],
  }));
}

/**
 * Single-ring fallback when no windowed quota is trustworthy: the worst
 * consumed share across every snapshot, so the dial still answers "how close
 * is anything to running out". Null stays null.
 */
export function fallbackConsumedRatio(snapshots: QuotaViewModel[]): number | null {
  const remaining = pressure(snapshots ?? []);
  return remaining === null ? null : clampUnit(1 - remaining);
}

/**
 * Concentric layout for `count` rings inside the 50-unit viewBox. Two rings
 * keep the established 21/14 radii with 5-unit bands; a third ring only
 * appears with real data and tightens to 4-unit bands (21/16/11) so three
 * bands plus two gaps still clear the centre mark.
 */
export function ringGeometry(count: number): RingGeometry[] {
  if (count <= 0) return [];
  if (count === 1) return [{ tone: "outer", radius: 21, width: 5 }];
  if (count === 2) {
    return [
      { tone: "outer", radius: 21, width: 5 },
      { tone: "inner", radius: 14, width: 5 },
    ];
  }
  return [
    { tone: "outer", radius: 21, width: 4 },
    { tone: "middle", radius: 16, width: 4 },
    { tone: "inner", radius: 11, width: 4 },
  ];
}

/** Centre brand-mark size in CSS pixels for a given ring count. */
export function ringIconSize(count: number): number {
  return count >= 3 ? 15 : 19;
}
