import React from "react";
import ReactDOM from "react-dom/client";
import App from "../features/dashboard/App";
import { BubbleView } from "../features/bubble/BubbleView";
import { createMockBridge } from "../lib/mock";
import { isTauri, tauriBridge, windowMode } from "../lib/tauri";

const mode = windowMode();
document.body.dataset.window = mode;

// Plain-browser `vite dev` has no Tauri commands; preview with demo data so UI
// states stay reviewable. Production builds never import the mock bridge.
const bridge = !isTauri() && import.meta.env.DEV ? createMockBridge() : tauriBridge;

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {mode === "bubble" ? <BubbleView bridge={bridge} /> : <App bridge={bridge} />}
  </React.StrictMode>,
);
