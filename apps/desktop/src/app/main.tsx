import React, { useEffect, useRef, useState } from "react";
import ReactDOM from "react-dom/client";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import App from "../features/dashboard/App";
import { BubbleView } from "../features/bubble/BubbleView";
import { createMockBridge } from "../lib/mock";
import { clipCircle, cornerRadius } from "../lib/morph";
import { isTauri, tauriBridge, windowMode } from "../lib/tauri";

const mode = windowMode();
document.body.dataset.window = mode;
if (!isTauri()) document.body.dataset.preview = "true";

// Plain-browser `vite dev` has no Tauri commands; preview with demo data so UI
// states stay reviewable. Production builds never import the mock bridge.
const bridge = !isTauri() && import.meta.env.DEV ? createMockBridge() : tauriBridge;

const REVEAL_SPRING = { type: "spring", stiffness: 240, damping: 30, mass: 0.9 } as const;
const BUBBLE_SPRING = { type: "spring", stiffness: 300, damping: 26 } as const;

/**
 * Preview-only shell: the dashboard and the bubble live in one document, so
 * their switch plays the shared clip-path circle morph. The desktop app swaps
 * real windows instead and never mounts this.
 *
 * The dashboard grows out of the bubble's own circle (not the viewport
 * centre) and the bubble re-enters at that same spot on the way back, so both
 * directions share one spatial path — the property the two-window handoff in
 * `lib/morph.ts` reproduces with real windows.
 */
function PreviewShell() {
  const [view, setView] = useState<"dashboard" | "bubble">(() =>
    new URLSearchParams(window.location.search).get("window") === "bubble" ? "bubble" : "dashboard",
  );
  const reduce = useReducedMotion();
  const [origin, setOrigin] = useState(() => ({
    x: window.innerWidth / 2,
    y: window.innerHeight / 2,
    diameter: 48,
  }));
  const revealRadius = useRef(0);
  revealRadius.current = cornerRadius(
    origin.x,
    origin.y,
    window.innerWidth || 1280,
    window.innerHeight || 860,
  );

  useEffect(() => {
    const showBubble = () => {
      setView("bubble");
      window.history.pushState(null, "", "/?window=bubble");
    };
    const showDashboard = () => {
      // The preview bubble is centred by the grid, so that is its anchor.
      setOrigin({ x: window.innerWidth / 2, y: window.innerHeight / 2, diameter: 48 });
      setView("dashboard");
      window.history.pushState(null, "", "/");
    };
    const sync = () => {
      setView(
        new URLSearchParams(window.location.search).get("window") === "bubble"
          ? "bubble"
          : "dashboard",
      );
    };
    window.addEventListener("tokenowl:show-bubble", showBubble);
    window.addEventListener("tokenowl:show-dashboard", showDashboard);
    window.addEventListener("popstate", sync);
    return () => {
      window.removeEventListener("tokenowl:show-bubble", showBubble);
      window.removeEventListener("tokenowl:show-dashboard", showDashboard);
      window.removeEventListener("popstate", sync);
    };
  }, []);

  // Keep the body hooks (backdrop, chrome) in step with the morphed view.
  useEffect(() => {
    document.body.dataset.window = view === "bubble" ? "bubble" : "browser";
  }, [view]);

  const collapsed = clipCircle(origin, origin.diameter / 2);

  return (
    <AnimatePresence>
      {view === "dashboard" ? (
        <motion.div
          key="dashboard"
          style={{
            position: "fixed",
            inset: 0,
            // No outer scroller: the window frame is pinned to the viewport
            // and only the dashboard's own .content region scrolls, so the
            // titlebar and dock stay visible (a second scrollbar here made
            // the whole shell slide under the user's cursor).
            overflow: "hidden",
            padding: 26,
            zIndex: 2,
          }}
          initial={reduce ? { opacity: 0 } : { clipPath: collapsed, opacity: 0.35 }}
          animate={
            reduce
              ? { opacity: 1 }
              : { clipPath: clipCircle(origin, revealRadius.current * 1.02), opacity: 1 }
          }
          exit={reduce ? { opacity: 0 } : { clipPath: collapsed, opacity: 0.35 }}
          transition={reduce ? { duration: 0.15 } : REVEAL_SPRING}
        >
          <App bridge={bridge} />
        </motion.div>
      ) : (
        <motion.div
          key="bubble"
          style={{
            position: "fixed",
            inset: 0,
            display: "grid",
            placeItems: "center",
            zIndex: 1,
          }}
          initial={reduce ? { opacity: 0 } : { scale: 0.86, opacity: 0 }}
          animate={{ scale: 1, opacity: 1 }}
          exit={reduce ? { opacity: 0 } : { scale: 0.86, opacity: 0 }}
          transition={reduce ? { duration: 0.15 } : BUBBLE_SPRING}
        >
          <BubbleView bridge={bridge} />
        </motion.div>
      )}
    </AnimatePresence>
  );
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {isTauri() ? (
      mode === "bubble" ? (
        <BubbleView bridge={bridge} />
      ) : (
        <App bridge={bridge} />
      )
    ) : (
      <PreviewShell />
    )}
  </React.StrictMode>,
);
