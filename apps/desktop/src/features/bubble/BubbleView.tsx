import { useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { motion, useReducedMotion } from "motion/react";
import "../../styles/App.css";
import { copy } from "../../styles/copy";
import { overallHealth } from "../../lib/format";
import { fallbackConsumedRatio, selectBubbleRings } from "../../lib/rings";
import { isTauri, shell, tauriBridge } from "../../lib/tauri";
import { onMorphFromBubble, onMorphToBubble, type MorphAnchor } from "../../lib/morph";
import { QuotaRings } from "./components/QuotaRings";
import type { AccountHealth, AccountViewModel, DashboardBridge } from "../../types";

const DRAG_THRESHOLD_PX = 5;

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
  // so it replays a small settle-in from the circle the dashboard shrank
  // into. It must be OPAQUE from the first frame: a translucent entrance let
  // the shrinking panel show through and read as a ghost. The same moment
  // re-applies the window's tool-window style and size (the shell rewrites
  // window flags on show).
  const [exiting, setExiting] = useState(false);
  useEffect(() => {
    if (!isTauri()) return;
    return onMorphToBubble((anchor) => {
      setExiting(false);
      setCollapseAnchor(anchor);
      void shell.stabilizeBubbleWindow().catch(() => null);
      // The anchor only seeds the entrance; clearing it keeps later
      // re-renders from replaying the scale.
      window.setTimeout(() => setCollapseAnchor(null), 600);
    });
  }, []);

  // When the dashboard expands over the bubble, the bubble fades out under
  // the growing panel instead of being hidden mid-frame by the shell.
  useEffect(() => {
    if (!isTauri()) return;
    return onMorphFromBubble(() => setExiting(true));
  }, []);

  const primary =
    accounts.find((account) => account.status !== "error") ?? accounts[0] ?? undefined;
  // Rings answer "how much is already used": the shortest window outside, the
  // longest inside, a third only when a third distinct real window exists.
  const rings = selectBubbleRings(primary?.snapshots ?? []);
  const fallbackRatio =
    rings.length === 0 ? fallbackConsumedRatio(accounts.flatMap((account) => account.snapshots)) : null;
  const ringSummary =
    rings.length > 0
      ? copy.bubble.rings(
          rings
            .map((ring) =>
              ring.consumedRatio === null
                ? `${ring.label} ${copy.summary.unknown}`
                : `${ring.label} ${Math.round(ring.consumedRatio * 100)}%`,
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
            ? // Keyframes replay on every collapse: settle in from the circle
              // the dashboard shrank into, fully opaque from frame one.
              { scale: [1.08, 1], opacity: 1 }
            : exiting
              ? { opacity: 0 }
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
        <QuotaRings
          rings={rings}
          fallback={fallbackRatio}
          providerId={primary ? primary.providerId : null}
          reducedMotion={reduce ?? false}
        />
      </motion.button>
    </div>
  );
}
