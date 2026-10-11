import React, { useEffect, useState } from "react";
import ReactDOM from "react-dom/client";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import App from "../features/dashboard/App";
import { BubbleView } from "../features/bubble/BubbleView";
import { createMockBridge } from "../lib/mock";
import { isTauri, tauriBridge, windowMode } from "../lib/tauri";

const mode = windowMode();
document.body.dataset.window = mode;
if (!isTauri()) document.body.dataset.preview = "true";

// Plain-browser `vite dev` has no Tauri commands; preview with demo data so UI
// states stay reviewable. Production builds never import the mock bridge.
const bridge = !isTauri() && import.meta.env.DEV ? createMockBridge() : tauriBridge;

const PATH_EASE: [number, number, number, number] = [0.32, 0.72, 0, 1];

/**
 * Preview-only shell: the dashboard and the bubble live in one document, so
 * their switch plays the shared clip-path circle morph. The desktop app swaps
 * real windows instead and never mounts this.
 */
function PreviewShell() {
  const [view, setView] = useState<"dashboard" | "bubble">(() =>
    new URLSearchParams(window.location.search).get("window") === "bubble" ? "bubble" : "dashboard",
  );
  const reduce = useReducedMotion();

  useEffect(() => {
    const showBubble = () => {
      setView("bubble");
      window.history.pushState(null, "", "/?window=bubble");
    };
    const showDashboard = () => {
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

  return (
    <AnimatePresence mode="wait">
      {view === "dashboard" ? (
        <motion.div
          key="dashboard"
          style={{ height: "100%" }}
          initial={{ clipPath: "circle(26px at 50% 50%)", opacity: 0.3 }}
          animate={{ clipPath: "circle(141% at 50% 50%)", opacity: 1 }}
          exit={{ clipPath: "circle(26px at 50% 50%)", opacity: 0.3 }}
          transition={reduce ? { duration: 0 } : { duration: 0.38, ease: PATH_EASE }}
        >
          <App bridge={bridge} />
        </motion.div>
      ) : (
        <motion.div
          key="bubble"
          style={{ height: "100%" }}
          initial={{ scale: 0.5, opacity: 0 }}
          animate={{ scale: 1, opacity: 1 }}
          exit={{ scale: 0.5, opacity: 0 }}
          transition={reduce ? { duration: 0 } : { type: "spring", stiffness: 210, damping: 24 }}
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
