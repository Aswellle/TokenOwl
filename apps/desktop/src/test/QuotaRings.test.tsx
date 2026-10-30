import { StrictMode, type ReactNode } from "react";
import { describe, expect, it, beforeEach, afterEach, vi } from "vitest";
import { render, waitFor } from "@testing-library/react";
import { QuotaRings } from "../features/bubble/components/QuotaRings";
import { gsap } from "../lib/animation/gsap";
import type { RingDatum, RingTone } from "../lib/rings";

// The real icon pack ships emoji data as JSON imports that vitest's transform
// rejects; ring behaviour does not depend on the glyph.
vi.mock("@lobehub/icons", () => ({ ProviderIcon: () => null }));

const OUTER_LENGTH = 2 * Math.PI * 21;
const INNER_LENGTH = 2 * Math.PI * 14;

function ring(id: string, label: string, consumed: number | null, tone: RingTone): RingDatum {
  return { id, label, consumedRatio: consumed, tone };
}

function renderRings(
  rings: RingDatum[],
  options: {
    fallback?: number | null;
    reducedMotion?: boolean;
    wrapper?: typeof StrictMode;
    morphPhase?: "idle" | "settle" | "exit";
    morphToken?: number;
  } = {},
) {
  const ui: ReactNode = (
    <QuotaRings
      rings={rings}
      fallback={options.fallback ?? null}
      providerId="openai:acc"
      reducedMotion={options.reducedMotion ?? false}
      morphPhase={options.morphPhase ?? "idle"}
      morphToken={options.morphToken ?? 0}
    />
  );
  return render(options.wrapper ? <options.wrapper>{ui}</options.wrapper> : ui);
}

function geometry(): SVGGElement {
  const node = document.querySelector<SVGGElement>(".quota-rings__geometry");
  if (!node) throw new Error("missing ring geometry");
  return node;
}

function offsetOf(selector: string): number {
  const node = document.querySelector(selector);
  if (!node) throw new Error(`missing ${selector}`);
  return Number(gsap.getProperty(node, "strokeDashoffset"));
}

function valueNode(tone: RingTone): SVGCircleElement {
  const node = document.querySelector<SVGCircleElement>(`.quota-rings__value--${tone}`);
  if (!node) throw new Error(`missing ${tone} value arc`);
  return node;
}

describe("QuotaRings", () => {
  beforeEach(() => {
    // Durations are real; the global time scale keeps the suite fast while
    // still exercising the tween machinery end to end.
    gsap.globalTimeline.timeScale(20);
  });

  afterEach(() => {
    gsap.globalTimeline.timeScale(1);
    gsap.globalTimeline.clear();
  });

  it("renders one track and one value arc per dial with position tones", () => {
    renderRings([ring("a", "5h", 0.2, "outer"), ring("b", "7d", 0.5, "inner")]);
    expect(document.querySelectorAll(".quota-rings__track")).toHaveLength(2);
    expect(valueNode("outer").getAttribute("r")).toBe("21");
    expect(valueNode("inner").getAttribute("r")).toBe("14");
    expect(document.querySelector('[data-ring-tone="outer"]')?.getAttribute("data-ring-consumed")).toBe(
      "0.200",
    );
  });

  it("animates each ring to its consumed share on first sight", async () => {
    renderRings([ring("a", "5h", 0.2, "outer"), ring("b", "7d", 0.5, "inner")]);
    await waitFor(() => {
      expect(offsetOf(".quota-rings__value--outer")).toBeCloseTo(OUTER_LENGTH * 0.8, 1);
      expect(offsetOf(".quota-rings__value--inner")).toBeCloseTo(INNER_LENGTH * 0.5, 1);
    });
  });

  it("settles immediately under reduced motion without leaving tweens", async () => {
    renderRings([ring("a", "5h", 0.2, "outer")], { reducedMotion: true });
    await waitFor(() =>
      expect(offsetOf(".quota-rings__value--outer")).toBeCloseTo(OUTER_LENGTH * 0.8, 2),
    );
    expect(gsap.getTweensOf(valueNode("outer"))).toHaveLength(0);
    expect(document.querySelector("[data-reduced-motion]")?.getAttribute("data-reduced-motion")).toBe(
      "true",
    );
  });

  it("tweens from the current value to the new one instead of restarting from empty", async () => {
    const { rerender } = renderRings([ring("a", "5h", 0.2, "outer")]);
    await waitFor(() => expect(offsetOf(".quota-rings__value--outer")).toBeCloseTo(OUTER_LENGTH * 0.8, 1));

    rerender(
      <QuotaRings
        rings={[ring("a", "5h", 0.6, "outer")]}
        fallback={null}
        providerId="openai:acc"
        reducedMotion={false}
      />,
    );
    // A refresh must never look like a reset: the arc travels from 80% of the
    // dial, not from a full empty dial.
    const midFlight = offsetOf(".quota-rings__value--outer");
    expect(midFlight).toBeLessThan(OUTER_LENGTH);
    await waitFor(() => expect(offsetOf(".quota-rings__value--outer")).toBeCloseTo(OUTER_LENGTH * 0.4, 1));
  });

  it("keeps only the newest tween when updates arrive back to back", async () => {
    const { rerender } = renderRings([ring("a", "5h", 0.1, "outer")]);
    await waitFor(() => expect(offsetOf(".quota-rings__value--outer")).toBeCloseTo(OUTER_LENGTH * 0.9, 1));

    rerender(
      <QuotaRings rings={[ring("a", "5h", 0.7, "outer")]} fallback={null} providerId="openai:acc" reducedMotion={false} />,
    );
    rerender(
      <QuotaRings rings={[ring("a", "5h", 0.35, "outer")]} fallback={null} providerId="openai:acc" reducedMotion={false} />,
    );

    await waitFor(() => expect(offsetOf(".quota-rings__value--outer")).toBeCloseTo(OUTER_LENGTH * 0.65, 1));
    expect(gsap.getTweensOf(valueNode("outer")).length).toBeLessThanOrEqual(1);
  });

  it("removes every tween when the bubble unmounts", async () => {
    const { unmount } = renderRings([ring("a", "5h", 0.2, "outer")]);
    await waitFor(() => expect(offsetOf(".quota-rings__value--outer")).toBeCloseTo(OUTER_LENGTH * 0.8, 1));
    const node = valueNode("outer");
    unmount();
    expect(gsap.getTweensOf(node)).toHaveLength(0);
  });

  it("survives StrictMode's double mount without duplicate tweens", async () => {
    renderRings([ring("a", "5h", 0.2, "outer"), ring("b", "7d", 0.5, "inner")], {
      wrapper: StrictMode,
    });
    await waitFor(() => {
      expect(offsetOf(".quota-rings__value--outer")).toBeCloseTo(OUTER_LENGTH * 0.8, 1);
      expect(offsetOf(".quota-rings__value--inner")).toBeCloseTo(INNER_LENGTH * 0.5, 1);
    });
    expect(gsap.getTweensOf(valueNode("outer")).length).toBeLessThanOrEqual(1);
    expect(gsap.getTweensOf(valueNode("inner")).length).toBeLessThanOrEqual(1);
  });

  it("draws no value arc for unknown data", () => {
    renderRings([ring("a", "5h", null, "outer")]);
    expect(document.querySelectorAll(".quota-rings__value")).toHaveLength(0);
    expect(document.querySelectorAll(".quota-rings__track")).toHaveLength(1);
  });

  it("fills the dial and butts the seam at 100% consumed", async () => {
    renderRings([ring("b", "7d", 1, "inner")]);
    await waitFor(() => expect(offsetOf(".quota-rings__value--inner")).toBeCloseTo(0, 1));
    expect(valueNode("inner").getAttribute("stroke-linecap")).toBe("butt");
  });

  it("falls back to a single outer dial when no window is trustworthy", () => {
    renderRings([], { fallback: 0.75 });
    expect(document.querySelectorAll(".quota-rings__track")).toHaveLength(1);
    expect(valueNode("outer").getAttribute("r")).toBe("21");
    expect(document.querySelector('[data-ring-id="fallback"]')).not.toBeNull();
  });

  it("compresses the rings during the expand exit", async () => {
    renderRings([ring("a", "5h", 0.2, "outer")], { morphPhase: "exit" });
    await waitFor(() => {
      expect(Number(gsap.getProperty(geometry(), "scale"))).toBeCloseTo(0.94, 2);
      expect(Number(gsap.getProperty(geometry(), "opacity"))).toBeCloseTo(0.5, 2);
    });
  });

  it("settles the rings back out on every collapse, replaying per token", async () => {
    const { rerender } = renderRings([ring("a", "5h", 0.2, "outer")], {
      morphPhase: "settle",
      morphToken: 1,
    });
    await waitFor(() => {
      expect(Number(gsap.getProperty(geometry(), "scale"))).toBeCloseTo(1, 2);
      expect(Number(gsap.getProperty(geometry(), "opacity"))).toBeCloseTo(1, 2);
    });

    rerender(
      <QuotaRings
        rings={[ring("a", "5h", 0.2, "outer")]}
        fallback={null}
        providerId="openai:acc"
        reducedMotion={false}
        morphPhase="settle"
        morphToken={2}
      />,
    );
    await waitFor(() => expect(Number(gsap.getProperty(geometry(), "scale"))).toBeCloseTo(1, 2));
    expect(gsap.getTweensOf(geometry()).length).toBeLessThanOrEqual(1);
  });

  it("skips the morph beat under reduced motion", async () => {
    renderRings([ring("a", "5h", 0.2, "outer")], {
      morphPhase: "exit",
      reducedMotion: true,
    });
    await waitFor(() => expect(Number(gsap.getProperty(geometry(), "scale"))).toBeCloseTo(1, 2));
    expect(gsap.getTweensOf(geometry())).toHaveLength(0);
  });

  it("leaves the ring group untouched while idle", () => {
    renderRings([ring("a", "5h", 0.2, "outer")]);
    expect(gsap.getTweensOf(geometry())).toHaveLength(0);
  });

  it("renders the solo owl when no provider is configured", () => {
    render(
      <QuotaRings rings={[]} fallback={null} providerId={null} reducedMotion={false} />,
    );
    expect(document.querySelector(".bubble__mark--solo")).not.toBeNull();
    expect(document.querySelectorAll(".quota-rings__track")).toHaveLength(0);
  });
});
