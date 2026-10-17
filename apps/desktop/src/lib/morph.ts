import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { isTauri } from "./tauri";

/**
 * The bubble and the dashboard are two separate native windows, so the
 * "morph" is a handoff, not a shared-element transition: Rust reports where
 * the bubble sits inside the dashboard's client area, and the dashboard's
 * window material grows out of (or collapses back into) that circle while the
 * bubble fades in the opposite direction. The two animations overlap by design
 * — the previous implementation hid the bubble first and showed the dashboard
 * after, which left at least one frame with neither surface on screen. That
 * gap is what read as a hard cut.
 */
export interface MorphAnchor {
  /** Bubble center, in dashboard client CSS pixels. */
  x: number;
  y: number;
  /** Bubble diameter, in CSS pixels. */
  diameter: number;
}

/** Payload shape of the `window:morph-from-bubble` / `window:morph-to-bubble` events. */
interface MorphAnchorPayload {
  x: number;
  y: number;
  diameter: number;
}

/**
 * Radius of a circle centred on `(x, y)` that reaches every corner of a
 * `width × height` box — the smallest clip radius that reveals the whole
 * window. Growing to exactly this value (instead of a hand-tuned percentage)
 * keeps the expansion correct at every window size and monitor layout.
 */
export function cornerRadius(x: number, y: number, width: number, height: number): number {
  const dx = Math.max(x, width - x);
  const dy = Math.max(y, height - y);
  return Math.hypot(dx, dy);
}

/** `circle()` clip-path revealing a circle of `radius` around the anchor. */
export function clipCircle(anchor: { x: number; y: number }, radius: number): string {
  return `circle(${Math.max(0, radius)}px at ${anchor.x}px ${anchor.y}px)`;
}

/**
 * Subscribes to the expand-from-bubble anchor and returns it once received.
 *
 * The listener is registered immediately on mount because Rust emits the event
 * in the same turn it shows the window — a `useEffect`-registered listener in
 * the component that *uses* the anchor would miss it. `null` means "no morph":
 * the dashboard was opened by the tray, on first launch, or the event arrived
 * after the animation window had already elapsed.
 */
export function useMorphFromBubble(): MorphAnchor | null {
  const [anchor, setAnchor] = useState<MorphAnchor | null>(null);

  useEffect(() => {
    if (!isTauri()) return;
    let active = true;
    const unlisten = listen<MorphAnchorPayload>("window:morph-from-bubble", (event) => {
      if (!active) return;
      const { x, y, diameter } = event.payload;
      if (Number.isFinite(x) && Number.isFinite(y) && Number.isFinite(diameter)) {
        setAnchor({ x, y, diameter });
      }
    }).catch(() => null);
    return () => {
      active = false;
      void unlisten.then((dispose) => dispose?.());
    };
  }, []);

  return anchor;
}

/**
 * Registers a one-shot listener for the collapse anchor. The bubble window
 * mounts `BubbleView` long before a collapse happens, so an imperative
 * subscription (rather than a hook) is what the caller actually needs.
 */
export function onMorphToBubble(handler: (anchor: MorphAnchor) => void): () => void {
  if (!isTauri()) return () => {};
  let disposed = false;
  const unlisten = listen<MorphAnchorPayload>("window:morph-to-bubble", (event) => {
    if (disposed) return;
    const { x, y, diameter } = event.payload;
    if (Number.isFinite(x) && Number.isFinite(y) && Number.isFinite(diameter)) {
      handler({ x, y, diameter });
    }
  }).catch(() => null);
  return () => {
    disposed = true;
    void unlisten.then((dispose) => dispose?.());
  };
}
