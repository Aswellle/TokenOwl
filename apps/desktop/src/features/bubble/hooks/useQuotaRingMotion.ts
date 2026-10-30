/**
 * GSAP owns exactly one attribute of the rings: `stroke-dashoffset`.
 *
 * Two behaviours, deliberately different:
 * - first sight of a ring draws it from an empty dial to its real consumed
 *   share, staggered outward-in;
 * - later quota updates tween from the current visual value to the new one,
 *   never restarting from empty (a refresh must not look like a reset).
 *
 * Reduced motion skips tweening and writes the target directly. Every tween
 * lives in the `useGSAP` context, so unmounting the bubble reverts them and
 * React StrictMode's mount/cleanup/mount cycle cannot leave duplicates.
 */
import { useRef, type RefObject } from "react";
import { gsap, useGSAP } from "../../../lib/animation/gsap";
import type { RingDatum } from "../../../lib/rings";

const FIRST_REVEAL_SECONDS = 0.62;
const FIRST_REVEAL_STAGGER = 0.085;
const UPDATE_SECONDS = 0.45;
const EASE = "power3.out";

interface RingTarget {
  id: string;
  node: SVGCircleElement;
  /** dashoffset that represents the consumed share. */
  target: number;
  /** dashoffset of an empty dial (the full circumference). */
  full: number;
}

function collectTargets(scope: SVGSVGElement, dials: RingDatum[]): RingTarget[] {
  const consumedById = new Map(dials.map((dial) => [dial.id, dial.consumedRatio]));
  const targets: RingTarget[] = [];
  for (const node of scope.querySelectorAll<SVGCircleElement>(".quota-rings__value")) {
    const id = node.parentElement?.getAttribute("data-ring-id");
    if (!id) continue;
    const consumed = consumedById.get(id);
    if (consumed === null || consumed === undefined) continue;
    const length = Number(node.getAttribute("stroke-dasharray"));
    if (!Number.isFinite(length) || length <= 0) continue;
    targets.push({ id, node, target: length * (1 - consumed), full: length });
  }
  return targets;
}

/** Stable dependency: the visual identity of the dials, not their array. */
function signatureOf(dials: RingDatum[]): string {
  return dials
    .map((dial) => `${dial.id}:${dial.consumedRatio === null ? "?" : dial.consumedRatio.toFixed(4)}`)
    .join("|");
}

export interface QuotaRingMotionOptions {
  scope: RefObject<SVGSVGElement | null>;
  dials: RingDatum[];
  reducedMotion: boolean;
}

export function useQuotaRingMotion({ scope, dials, reducedMotion }: QuotaRingMotionOptions) {
  // Rings already shown at least once; only new ones replay the reveal.
  const revealed = useRef(new Set<string>());
  const signature = signatureOf(dials);

  useGSAP(
    () => {
      const root = scope.current;
      if (!root) return;
      const targets = collectTargets(root, dials);
      if (targets.length === 0) {
        revealed.current.clear();
        return;
      }

      if (reducedMotion) {
        for (const target of targets) {
          gsap.set(target.node, { strokeDashoffset: target.target });
          revealed.current.add(target.id);
        }
        return;
      }

      let revealIndex = 0;
      for (const target of targets) {
        if (revealed.current.has(target.id)) {
          // A known ring follows the data; overwrite stops a still-running
          // reveal or previous update from fighting the newest value.
          gsap.to(target.node, {
            strokeDashoffset: target.target,
            duration: UPDATE_SECONDS,
            ease: EASE,
            overwrite: "auto",
          });
        } else {
          // Mark before animating: a refresh that lands on the frame the
          // reveal finishes must tween from the current value, not restart
          // from an empty dial because `onComplete` had not fired yet.
          revealed.current.add(target.id);
          gsap.fromTo(
            target.node,
            { strokeDashoffset: target.full },
            {
              strokeDashoffset: target.target,
              duration: FIRST_REVEAL_SECONDS,
              ease: EASE,
              delay: revealIndex * FIRST_REVEAL_STAGGER,
              overwrite: "auto",
            },
          );
          revealIndex += 1;
        }
      }
    },
    { scope, dependencies: [signature, reducedMotion] },
  );
}


export type BubbleMorphPhase = "idle" | "settle" | "exit";

export interface BubbleMorphBeatOptions {
  scope: RefObject<SVGSVGElement | null>;
  phase: BubbleMorphPhase;
  /** Increments on every collapse so repeated settles replay. */
  token: number;
  reducedMotion: boolean;
}

/**
 * The bubble's local half of the window morph. Rust owns the OS window
 * region; this only breathes the ring group while that region animation runs:
 * the rings compress as the panel grows over them, and settle outward again
 * when the panel collapses back. `token` makes each collapse replay even when
 * the phase string is unchanged.
 */
export function useBubbleMorphBeat({ scope, phase, token, reducedMotion }: BubbleMorphBeatOptions) {
  useGSAP(
    () => {
      const geometry = scope.current?.querySelector(".quota-rings__geometry");
      if (!geometry) return;
      if (reducedMotion || phase === "idle") {
        gsap.set(geometry, { scale: 1, opacity: 1 });
        return;
      }
      if (phase === "exit") {
        gsap.to(geometry, {
          scale: 0.94,
          opacity: 0.5,
          duration: 0.18,
          ease: "power1.in",
          overwrite: "auto",
        });
        return;
      }
      gsap.fromTo(
        geometry,
        { scale: 0.94, opacity: 0.55 },
        { scale: 1, opacity: 1, duration: 0.26, ease: "power2.out", overwrite: "auto" },
      );
    },
    { scope, dependencies: [phase, token, reducedMotion] },
  );
}
