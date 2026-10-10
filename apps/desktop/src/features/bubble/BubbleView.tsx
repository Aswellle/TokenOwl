import { useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import "../../styles/App.css";
import { copy } from "../../styles/copy";
import { IconOwl } from "../../components/icons";
import { overallHealth, pressure } from "../../lib/format";
import { shell, tauriBridge } from "../../lib/tauri";
import type { AccountHealth, AccountViewModel, DashboardBridge } from "../../types";

const RADIUS = 22.5;
const CIRCUMFERENCE = 2 * Math.PI * RADIUS;
const DRAG_THRESHOLD_PX = 5;

/**
 * The compact always-on-top floating bubble. Lives in its own 56×56 window:
 * click opens the dashboard, drag moves the window (position persists in Rust).
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
      void unlisten.then((dispose) => dispose?.());
    };
  }, [bridge]);

  // Persist the window position after a drag settles.
  useEffect(() => {
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
        void getCurrentWindow()
          .startDragging()
          .catch(() => null);
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
  const ratio = pressure(accounts.flatMap((account) => account.snapshots));
  const Icon = IconOwl;

  return (
    <div className="bubwin">
      <button
        className={`bubble bubble--${status}`}
        type="button"
        onPointerDown={scheduleDrag}
        onClick={() => {
          if (dragging.current) {
            dragging.current = false;
            return;
          }
          void shell.openDashboard().catch(() => null);
        }}
        aria-label={`${copy.brand}：${copy.health[status] ?? status}。${copy.bubble.openDashboard}`}
        title={copy.bubble.openDashboard}
      >
        <svg className="bubble__ring" viewBox="0 0 50 50" aria-hidden="true">
          <circle className="bubble__ring-track" cx="25" cy="25" r={RADIUS} />
          {ratio !== null && (
            <circle
              className="bubble__ring-value"
              cx="25"
              cy="25"
              r={RADIUS}
              strokeDasharray={CIRCUMFERENCE}
              strokeDashoffset={CIRCUMFERENCE * (1 - ratio)}
            />
          )}
        </svg>
        <span className="bubble__mark">
          <Icon size={26} strokeWidth={1.5} />
        </span>
      </button>
    </div>
  );
}
