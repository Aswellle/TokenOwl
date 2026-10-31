/**
 * The bubble's activity rings: up to three concentric quota dials plus the
 * centre mark.
 *
 * Visual language follows the reference's Activity Rings — thick bands, a
 * clear dark gap between them, round progress caps, and a bright gradient over
 * a dimmer same-hue track — while the data stays TokenOwl's own: one ring per
 * real, distinct, trusted quota window (shortest outside, longest inside).
 * Nothing here invents a third window; `selectBubbleRings` decides.
 *
 * Ownership: this component owns the SVG's static attributes (dasharray,
 * radius, width, gradient). `stroke-dashoffset` is owned by the GSAP timeline
 * in `useQuotaRingMotion` — no CSS transition may touch it.
 */
import { useId, useRef } from "react";
import { IconOwl } from "../../../components/icons";
import { ProviderIcon } from "../../../components/ProviderIcon";
import { ringGeometry, ringIconSize, type RingDatum } from "../../../lib/rings";
import { useBubbleMorphBeat, useQuotaRingMotion, type BubbleMorphPhase } from "../hooks/useQuotaRingMotion";

const VIEWBOX = 50;
const CENTER = VIEWBOX / 2;

/** Same-hue gradient endpoints per ring position (design tokens, not data). */
const GRADIENTS: Record<RingDatum["tone"], readonly [string, string]> = {
  outer: ["#E83E74", "#F45B9A"],
  middle: ["#B6F34A", "#DFFF62"],
  inner: ["#43C9EC", "#6DEBCB"],
};

function circumference(radius: number): number {
  return 2 * Math.PI * radius;
}

export interface QuotaRingsProps {
  /** Real windowed rings, already selected and tone-assigned. */
  rings: RingDatum[];
  /** Single fallback dial when no windowed quota is trustworthy. */
  fallback: number | null;
  /** Provider of the primary account; null renders the solo owl. */
  providerId: string | null;
  reducedMotion: boolean;
  /** Local morph beat driven by the shell's morph events. */
  morphPhase?: BubbleMorphPhase;
  /** Bumped per collapse so repeated settles replay. */
  morphToken?: number;
}

/**
 * Normalises the two inputs into the dials actually drawn: real windows when
 * present, otherwise at most one fallback dial, otherwise none.
 */
export function bubbleDials(rings: RingDatum[], fallback: number | null): RingDatum[] {
  if (rings.length > 0) return rings;
  if (fallback === null) return [];
  return [{ id: "fallback", label: "", consumedRatio: fallback, tone: "outer" }];
}

export function QuotaRings({
  rings,
  fallback,
  providerId,
  reducedMotion,
  morphPhase = "idle",
  morphToken = 0,
}: QuotaRingsProps) {
  const instanceId = useId();
  const svgRef = useRef<SVGSVGElement>(null);
  const dials = bubbleDials(rings, fallback);
  const geometry = ringGeometry(dials.length);
  const iconSize = ringIconSize(dials.length);
  useQuotaRingMotion({ scope: svgRef, dials, reducedMotion });
  useBubbleMorphBeat({ scope: svgRef, phase: morphPhase, token: morphToken, reducedMotion });

  return (
    <>
      <svg
        ref={svgRef}
        className="quota-rings"
        viewBox={`0 0 ${VIEWBOX} ${VIEWBOX}`}
        aria-hidden="true"
        data-ring-count={dials.length}
        data-morph-phase={morphPhase}
        data-reduced-motion={reducedMotion ? "true" : "false"}
      >
        <defs>
          {dials.map((ring) => (
            <linearGradient
              key={ring.tone}
              id={`${instanceId}-${ring.tone}`}
              x1="0%"
              y1="0%"
              x2="100%"
              y2="100%"
            >
              <stop offset="0%" stopColor={GRADIENTS[ring.tone][0]} />
              <stop offset="100%" stopColor={GRADIENTS[ring.tone][1]} />
            </linearGradient>
          ))}
        </defs>
        <g className="quota-rings__geometry">
          {dials.map((ring, index) => {
            const { radius, width } = geometry[index];
            const length = circumference(radius);
            const consumed = ring.consumedRatio;
            return (
              <g
                key={ring.id}
                data-ring-id={ring.id}
                data-ring-tone={ring.tone}
                data-ring-consumed={consumed === null ? "unknown" : consumed.toFixed(3)}
              >
                <circle
                  className={`quota-rings__track quota-rings__track--${ring.tone}`}
                  cx={CENTER}
                  cy={CENTER}
                  r={radius}
                  strokeWidth={width}
                />
                {/* Zero and unknown draw no arc at all: a round cap at zero
                    would leave a stray dot on an otherwise empty dial. */}
                {consumed !== null && consumed > 0 && (
                  <circle
                    className={`quota-rings__value quota-rings__value--${ring.tone}`}
                    cx={CENTER}
                    cy={CENTER}
                    r={radius}
                    strokeWidth={width}
                    stroke={`url(#${instanceId}-${ring.tone})`}
                    strokeDasharray={length}
                    /* stroke-dashoffset is intentionally absent: GSAP owns it
                       (see useQuotaRingMotion) so a data change never jumps. */
                    /* Butt caps at full consumption: two round caps meeting at
                       12 o'clock would bulge the seam. */
                    strokeLinecap={consumed >= 1 ? "butt" : "round"}
                  />
                )}
              </g>
            );
          })}
        </g>
      </svg>
      <span
        className={`bubble__mark${providerId === null ? " bubble__mark--solo" : ""}`}
        data-ring-count={dials.length}
      >
        {providerId === null ? (
          <IconOwl size={46} strokeWidth={1.6} />
        ) : (
          <ProviderIcon providerId={providerId} size={iconSize} />
        )}
      </span>
    </>
  );
}
