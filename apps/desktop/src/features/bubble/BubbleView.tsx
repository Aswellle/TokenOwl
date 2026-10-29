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
import { onMorphToBubble, type MorphAnchor } from "../../lib/morph";
import type { AccountHealth, AccountViewModel, DashboardBridge, QuotaViewModel } from "../../types";

const DRAG_THRESHOLD_PX = 5;
// Apple-Watch-style rings filling the 50×50 viewBox, layered with a clean
// 2-unit gap between the bands: the outer band spans 18.5–23.5 (1.5 units of
// breathing room to the button's edge) and the inner 11.5–16.5, leaving a
// radius-11.5 hole for the brand mark. Two adjacent bands read as one blob;
// the gap is what makes them read as two separate dials. The 5-unit band
// width lives in the `.bubble__ring-*` CSS.
const RING_RADII = [21, 14];

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
  // Set when the dashboard collapses into this bubble: the bubble then scales
  // up from the ring it is replacing instead of popping in at full size.
  const [collapseAnchor, setCollapseAnchor] = useState<MorphAnchor | null>(null);
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

  // First render covers the show paths that bypass the morph (e.g. a future
  // direct-show): pin the window shape the same way the morph path does.
  useEffect(() => {
    if (!isTauri() || phase !== "ready") return;
    void shell.stabilizeBubbleWindow().catch(() => null);
  }, [phase]);

  // A collapse from the dashboard arrives as an event on this window. The
  // bubble is already visible at this point (Rust shows it before emitting),
  // so it starts scaled up from the dashboard's outgoing circle and settles
  // into place — the reverse of the expansion the dashboard plays. The same
  // moment re-applies the window's tool-window style and size: the shell
  // rewrites window flags on show, and this lands after those writes settle.
  useEffect(() => {
    if (!isTauri()) return;
    return onMorphToBubble((anchor) => {
      setCollapseAnchor(anchor);
      void shell.stabilizeBubbleWindow().catch(() => null);
      // The anchor only seeds the entrance; clearing it keeps later
      // re-renders from replaying the scale.
      window.setTimeout(() => setCollapseAnchor(null), 600);
    });
  }, []);

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
        initial={reduce ? false : { scale: 0.86, opacity: 0 }}
        animate={
          collapseAnchor
            ? // Keyframes replay on every collapse: grow from the circle the
              // dashboard just shrank into, instead of popping at full size.
              { scale: [Math.max(1, collapseAnchor.diameter / 48), 1], opacity: [0.4, 1] }
            : { scale: 1, opacity: 1 }
        }
        transition={reduce ? { duration: 0 } : { type: "spring", stiffness: 300, damping: 26, mass: 0.8 }}
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
          {rings.length === 0 && fallbackRatio !== null && fallbackRatio > 0 && (
            <g>
              <circle className="bubble__ring-track" cx="25" cy="25" r={RING_RADII[0]} />
              <circle
                className="bubble__ring-value bubble__ring-value--0"
                cx="25"
                cy="25"
                r={RING_RADII[0]}
                strokeDasharray={circumference(RING_RADII[0])}
                strokeDashoffset={circumference(RING_RADII[0]) * (1 - Math.min(fallbackRatio, 1))}
              />
            </g>
          )}
          {rings.map((ring, index) => {
            const radius = RING_RADII[index];
            const ratio = ring.ratio;
            return (
              <g key={ring.label}>
                <circle className="bubble__ring-track" cx="25" cy="25" r={radius} />
                {/* Only a measurable remaining share draws an arc: at 0 or
                    unknown the unlit track stands alone — a round line cap
                    would otherwise leave a stray dot on an empty dial. */}
                {ratio !== null && ratio > 0 && (
                  <circle
                    className={`bubble__ring-value bubble__ring-value--${index}`}
                    cx="25"
                    cy="25"
                    r={radius}
                    strokeDasharray={circumference(radius)}
                    strokeDashoffset={circumference(radius) * (1 - Math.min(ratio, 1))}
                  />
                )}
              </g>
            );
          })}
        </svg>
        <span className={`bubble__mark${primary ? "" : " bubble__mark--solo"}`}>
          {primary ? (
            <ProviderIcon providerId={primary.providerId} size={19} />
          ) : (
            /* No accounts yet: the owl alone on the dark dial, sized to sit
               inside the circle without a transform hack. It breathes softly
               so the empty state reads as "alive, waiting", not broken. */
            <IconOwl size={50} strokeWidth={1.6} />
          )}
        </span>
      </motion.button>
    </div>
  );
}
