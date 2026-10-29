import { listen } from "@tauri-apps/api/event";
import { isTauri } from "./tauri";

/**
 * The bubble and the dashboard are two separate native windows, so the
 * "morph" is a handoff, not a shared-element transition: the Rust shell owns
 * the OS window-region animation (it must clip the acrylic material, which no
 * CSS effect can reach) and reports the anchor circle to whichever window
 * needs to coordinate its own fade — the bubble fades out on expand and
 * settles in on collapse while the panel region grows or shrinks.
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

/**
 * Registers a listener for the expand anchor on the bubble side: the bubble
 * plays its own exit fade while the panel grows over it, so its hide is never
 * a hard cut.
 */
export function onMorphFromBubble(handler: (anchor: MorphAnchor) => void): () => void {
  if (!isTauri()) return () => {};
  let disposed = false;
  const unlisten = listen<MorphAnchorPayload>("window:morph-from-bubble", (event) => {
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
