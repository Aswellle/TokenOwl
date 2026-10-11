import { useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { motion, useReducedMotion } from "motion/react";
import "../../styles/App.css";
import { copy } from "../../styles/copy";
import { IconOwl } from "../../components/icons";
import { ProviderIcon } from "../../components/ProviderIcon";
import { overallHealth, pressure, quotaRatio, quotaWindowTag } from "../../lib/format";
import { isTauri, shell, tauriBridge } from "../../lib/tauri";
import type { AccountHealth, AccountViewModel, DashboardBridge, QuotaViewModel } from "../../types";

const DRAG_THRESHOLD_PX = 5;
// Apple-Watch-style rings filling the 50×50 viewBox: the outer band runs flush
// to the bubble edge (no rim ring), the bands sit adjacent at width 5, and the
// hole left over (radius 13.5) is the brand icon's space.
const RING_RADII = [21.5, 16.5];
const RING_BAND = 5;

interface Ring {
  label: string;
  ratio: number | null;
}

function circumference(radius: number): number {
  return 2 * Math.PI * radius;
}

/** Window span in hours from the quota window's own start/end timestamps. */
function spanHours(quota: QuotaViewModel): number {
  const start = quota.window?.startsAt ? Date.parse(String(quota.window.startsAt)) : NaN;
  const end = quota.window?.endsAt ? Date.parse(String(quota.window.endsAt)) : NaN;
  if (!Number.isFinite(start) || !Number.isFinite(end) || end <= start) return 0;
  return (end - start) / 3_600_000;
}

/**
 * The dual activity rings of the collapsed bubble: one ring per quota window
 * of the primary account (shortest window outside, longest inside), in the
 * Apple-Watch style. Accounts without windowed quotas fall back to a single
 * worst-pressure ring.
 */
function ringsFor(account: AccountViewModel | undefined): Ring[] {
  if (!account) return [];
  const windowed = account.snapshots
    .filter((quota) => {
      const trusted = quota.status === "available" || quota.status === "partial";
      return trusted && quotaWindowTag(quota) !== null;
    })
    .sort(
      (left, right) =>
        spanHours(left) - spanHours(right),
    )
    .slice(0, RING_RADII.length)
    .map((quota) => ({
      label: quotaWindowTag(quota) ?? "",
      ratio: quotaRatio(quota),
    }));
  return windowed;
}

/**
 * The compact always-on-top floating bubble. Lives in its own 56×56 window:
 * click opens the dashboard, drag moves the window (position snaps to the
 * nearest screen edge and persists in Rust).
 */
export function BubbleView({ bridge = tauriBridge }: { bridge?: DashboardBridge }) {
  const [accounts, setAccounts] = useState<AccountViewModel[]>([]);
  const [phase, setPhase] = useState<"loading" | "ready" | "offline">("loading");
  const persistTimer = useRef<number | null>(null);
  const dragging = useRef(false);

  useEffect(() => {
    let active = true;
    const load = () =>
      bridge
        .listAccounts()
        .then((loaded) => {
          if (!active) return;
          setAccounts(loaded);
          setPhase("ready");
        })
        .catch(() => {
          if (active) setPhase("offline");
        });
    void load();
    const unlisten = listen("quota:updated", load).catch(() => null);
    return () => {
      active = false;
      void unlisten.then((dispose) => { if (typeof dispose === "function") dispose(); });
    };
  }, [bridge]);

  // Persist the window position after a drag settles. The plain-browser
  // preview has no native window, so the move listener is desktop-only.
  useEffect(() => {
    if (!isTauri()) return;
    let active = true;
    const unlisten = getCurrentWindow()
      .onMoved(() => {
        if (persistTimer.current !== null) window.clearTimeout(persistTimer.current);
        persistTimer.current = window.setTimeout(() => {
          persistTimer.current = null;
          if (active) void shell.persistBubblePosition().catch(() => null);
        }, 400);
      })
      .catch(() => null);
    return () => {
      active = false;
      if (persistTimer.current !== null) {
        window.clearTimeout(persistTimer.current);
        persistTimer.current = null;
      }
      void unlisten.then((dispose) => dispose?.());
    };
  }, []);

  const scheduleDrag = (event: React.PointerEvent<HTMLButtonElement>) => {
    if (!event.isPrimary || event.button !== 0) return;
    const startX = event.clientX;
    const startY = event.clientY;
    dragging.current = false;
    const target = event.currentTarget;
    const onMove = (move: PointerEvent) => {
      const distance = Math.hypot(move.clientX - startX, move.clientY - startY);
      if (!dragging.current && distance > DRAG_THRESHOLD_PX) {
        dragging.current = true;
        target.removeEventListener("pointermove", onMove);
        if (!isTauri()) return; // nothing to drag in the browser preview
        try {
          void getCurrentWindow()
            .startDragging()
            .catch(() => null);
        } catch {
          // the window API is unavailable outside the desktop shell
        }
      }
    };
    target.addEventListener("pointermove", onMove);
    const onUp = () => {
      target.removeEventListener("pointermove", onMove);
      target.removeEventListener("pointerup", onUp);
    };
    target.addEventListener("pointerup", onUp, { once: true });
  };

  const status: AccountHealth = overallHealth(accounts, phase);
  const reduce = useReducedMotion();
  const primary =
    accounts.find((account) => account.status !== "error") ?? accounts[0] ?? undefined;
  const rings = ringsFor(primary);
  const fallbackRatio = rings.length === 0 ? pressure(accounts.flatMap((account) => account.snapshots)) : null;
  const ringSummary =
    rings.length > 0
      ? copy.bubble.rings(
          rings
            .map((ring) =>
              ring.ratio === null
                ? `${ring.label} ${copy.summary.unknown}`
                : `${ring.label} ${Math.round(ring.ratio * 100)}%`,
            )
            .join("，"),
        )
      : fallbackRatio !== null
        ? copy.bubble.rings(`${Math.round(fallbackRatio * 100)}%`)
        : copy.bubble.noWindow;

  // Hold the bubble back until the first account load settles: rendering the
  // no-data look during the initial fetch reads as a phantom empty state.
  if (phase === "loading") return null;

  return (
    <div className="bubwin">
      <motion.button
        className={`bubble bubble--${status}`}
        type="button"
        initial={reduce ? false : { scale: 0.4, opacity: 0 }}
        animate={{ scale: 1, opacity: 1 }}
        transition={{ type: "spring", stiffness: 190, damping: 22 }}
        whileHover={reduce ? undefined : { scale: 1.04 }}
        whileTap={reduce ? undefined : { scale: 0.94 }}
        onPointerDown={scheduleDrag}
        onClick={() => {
          if (dragging.current) {
            dragging.current = false;
            return;
          }
          if (!isTauri()) {
            // Browser preview: morph back to the dashboard in this document.
            window.dispatchEvent(new CustomEvent("tokenowl:show-dashboard"));
            return;
          }
          void shell.openDashboard().catch(() => null);
        }}
        aria-label={`${copy.brand}：${copy.health[status] ?? status}。${ringSummary}。${copy.bubble.openDashboard}`}
        title={`${ringSummary} · ${copy.bubble.openDashboard}`}
      >
        <svg className="bubble__rings" viewBox="0 0 50 50" aria-hidden="true">
          {(rings.length > 0 || fallbackRatio !== null) && (
            <circle
              className="bubble__ring-channel"
              cx="25"
              cy="25"
              r={(RING_RADII[0] + RING_RADII[1]) / 2}
              style={{ strokeWidth: RING_RADII[0] - RING_RADII[1] + RING_BAND }}
            />
          )}
          {rings.length === 0 && fallbackRatio !== null && (
            <g>
              <circle className="bubble__ring-track" cx="25" cy="25" r={RING_RADII[0]} />
              <circle
                className="bubble__ring-value bubble__ring-value--0"
                cx="25"
                cy="25"
                r={RING_RADII[0]}
                strokeDasharray={circumference(RING_RADII[0])}
                strokeDashoffset={circumference(RING_RADII[0]) * (1 - fallbackRatio)}
              />
            </g>
          )}
          {rings.map((ring, index) => {
            const radius = RING_RADII[index];
            const ratio = ring.ratio ?? 0;
            return (
              <g key={ring.label}>
                <circle className="bubble__ring-track" cx="25" cy="25" r={radius} />
                <circle
                  className={`bubble__ring-value bubble__ring-value--${index}`}
                  cx="25"
                  cy="25"
                  r={radius}
                  strokeDasharray={circumference(radius)}
                  strokeDashoffset={ratio === null ? circumference(radius) : circumference(radius) * (1 - ratio)}
                />
              </g>
            );
          })}
        </svg>
        <span className="bubble__mark">
          {primary ? (
            <ProviderIcon providerId={primary.providerId} size={19} />
          ) : (
            <IconOwl size={17} strokeWidth={1.6} />
          )}
        </span>
      </motion.button>
    </div>
  );
}
