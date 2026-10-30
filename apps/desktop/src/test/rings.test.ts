import { describe, expect, it } from "vitest";
import {
  MAX_BUBBLE_RINGS,
  fallbackConsumedRatio,
  quotaConsumedRatio,
  quotaSpanHours,
  ringGeometry,
  ringIconSize,
  selectBubbleRings,
} from "../lib/rings";
import type { QuotaViewModel } from "../types";

function quota(overrides: Partial<QuotaViewModel> = {}): QuotaViewModel {
  return {
    id: "q",
    metricName: "window",
    unit: "token",
    status: "available",
    ...overrides,
  };
}

/** A windowed quota whose span produces the expected tag ("5h", "7d", …). */
function windowed(
  id: string,
  spanHours: number,
  remaining: number | null,
  overrides: Partial<QuotaViewModel> = {},
): QuotaViewModel {
  const now = Date.now();
  return quota({
    id,
    metricName: `${spanHours}h window`,
    remaining,
    limit: 100,
    window: {
      type: "rolling",
      startsAt: new Date(now - spanHours * 3_600_000).toISOString(),
      endsAt: new Date(now).toISOString(),
    },
    ...overrides,
  });
}

describe("quotaConsumedRatio", () => {
  it("turns the remaining share into a consumed share", () => {
    expect(quotaConsumedRatio(quota({ remaining: 80, limit: 100 }))).toBeCloseTo(0.2);
  });

  it("derives from used when the provider omits remaining", () => {
    expect(quotaConsumedRatio(quota({ used: 20, limit: 100 }))).toBeCloseTo(0.2);
  });

  it("reports a fully consumed quota as 1 and an untouched one as 0", () => {
    expect(quotaConsumedRatio(quota({ remaining: 0, limit: 100 }))).toBe(1);
    expect(quotaConsumedRatio(quota({ remaining: 100, limit: 100 }))).toBe(0);
  });

  it("keeps unknown as null instead of pretending zero consumption", () => {
    expect(quotaConsumedRatio(quota({ limit: 0, remaining: 10 }))).toBeNull();
    expect(quotaConsumedRatio(quota({ limit: null, remaining: 10 }))).toBeNull();
    expect(quotaConsumedRatio(quota({ limit: undefined, remaining: 10 }))).toBeNull();
    expect(quotaConsumedRatio(quota({ limit: 100 }))).toBeNull();
  });

  it("clamps providers that report remaining above the limit or below zero", () => {
    // Bonus credit: remaining 150 of 100 is 100% remaining → 0% consumed.
    expect(quotaConsumedRatio(quota({ remaining: 150, limit: 100 }))).toBe(0);
    // Overdrawn: remaining -20 of 100 is 0% remaining → 100% consumed.
    expect(quotaConsumedRatio(quota({ remaining: -20, limit: 100 }))).toBe(1);
    expect(quotaConsumedRatio(quota({ used: 120, limit: 100 }))).toBe(1);
    expect(quotaConsumedRatio(quota({ used: -5, limit: 100 }))).toBe(0);
  });
});

describe("quotaSpanHours", () => {
  it("measures the span from the window's own timestamps", () => {
    expect(quotaSpanHours(windowed("a", 5, 50))).toBeCloseTo(5, 3);
  });

  it("returns 0 when the window carries no usable span", () => {
    expect(quotaSpanHours(quota({ metricName: "5 小时窗口" }))).toBe(0);
    const now = Date.now();
    expect(
      quotaSpanHours(
        quota({
          window: {
            startsAt: new Date(now).toISOString(),
            endsAt: new Date(now - 3_600_000).toISOString(),
          },
        }),
      ),
    ).toBe(0);
  });
});

describe("selectBubbleRings", () => {
  it("returns no rings without trusted windowed quotas", () => {
    expect(selectBubbleRings([])).toEqual([]);
    expect(selectBubbleRings([quota({ metricName: "余额", remaining: 5, limit: 10 })])).toEqual([]);
  });

  it("draws a single outer ring when only one window exists", () => {
    const rings = selectBubbleRings([windowed("a", 5, 80)]);
    expect(rings).toHaveLength(1);
    expect(rings[0]).toMatchObject({ id: "a", label: "5h", tone: "outer" });
    expect(rings[0].consumedRatio).toBeCloseTo(0.2);
  });

  it("places the shortest window outside and the longest inside", () => {
    const rings = selectBubbleRings([windowed("weekly", 168, 90), windowed("five", 5, 60)]);
    expect(rings.map((ring) => [ring.label, ring.tone])).toEqual([
      ["5h", "outer"],
      ["7d", "inner"],
    ]);
  });

  it("adds a middle ring only for a third distinct window, furthest from both ends", () => {
    const rings = selectBubbleRings([
      windowed("hourly", 1, 50),
      windowed("five", 5, 50),
      windowed("weekly", 168, 50),
    ]);
    expect(rings.map((ring) => [ring.label, ring.tone])).toEqual([
      ["1h", "outer"],
      ["5h", "middle"],
      ["7d", "inner"],
    ]);
  });

  it("keeps three rings when more windows exist, choosing the most separated middle", () => {
    const rings = selectBubbleRings([
      windowed("hourly", 1, 50),
      windowed("five", 5, 50),
      windowed("weekly", 168, 50),
      windowed("monthly", 720, 50),
    ]);
    expect(rings).toHaveLength(MAX_BUBBLE_RINGS);
    expect(rings.map((ring) => ring.label)).toEqual(["1h", "7d", "30d"]);
  });

  it("collapses duplicate window tags instead of drawing the same window twice", () => {
    const rings = selectBubbleRings([windowed("a", 5, 50), windowed("b", 5, 70)]);
    expect(rings).toHaveLength(1);
    expect(rings[0].id).toBe("a");
  });

  it("ignores windows the provider did not actually report", () => {
    const rings = selectBubbleRings([
      windowed("errored", 5, 50, { status: "error" }),
      windowed("missing", 168, 50, { status: "unavailable" }),
      windowed("ok", 168, 50, { status: "partial" }),
    ]);
    expect(rings.map((ring) => ring.id)).toEqual(["ok"]);
  });

  it("keeps an unknown ratio as null rather than 0", () => {
    const rings = selectBubbleRings([windowed("a", 5, null, { limit: null })]);
    expect(rings[0].consumedRatio).toBeNull();
  });

  it("falls back to the provider's window naming when timestamps are absent", () => {
    const rings = selectBubbleRings([quota({ id: "named", metricName: "5 小时窗口", remaining: 50, limit: 100 })]);
    expect(rings[0].label).toBe("5h");
  });
});

describe("fallbackConsumedRatio", () => {
  it("uses the worst remaining share across every snapshot", () => {
    expect(
      fallbackConsumedRatio([
        quota({ remaining: 80, limit: 100 }),
        quota({ remaining: 20, limit: 100 }),
      ]),
    ).toBeCloseTo(0.8);
  });

  it("stays null when nothing is measurable", () => {
    expect(fallbackConsumedRatio([])).toBeNull();
    expect(fallbackConsumedRatio([quota({ remaining: null, limit: null })])).toBeNull();
  });
});

describe("ringGeometry", () => {
  it("lays out one and two rings on the established radii", () => {
    expect(ringGeometry(0)).toEqual([]);
    expect(ringGeometry(1)).toEqual([{ tone: "outer", radius: 21, width: 5 }]);
    expect(ringGeometry(2)).toEqual([
      { tone: "outer", radius: 21, width: 5 },
      { tone: "inner", radius: 14, width: 5 },
    ]);
  });

  it("keeps a visible gap between bands", () => {
    const two = ringGeometry(2);
    expect(two[0].radius - two[1].radius - two[0].width).toBe(2);

    const three = ringGeometry(3);
    expect(three[0].radius - three[1].radius - three[0].width).toBe(1);
    expect(three[1].radius - three[2].radius - three[1].width).toBe(1);
  });

  it("tightens bands for a real third ring and shrinks the centre mark", () => {
    expect(ringGeometry(3).map((ring) => [ring.tone, ring.radius, ring.width])).toEqual([
      ["outer", 21, 4],
      ["middle", 16, 4],
      ["inner", 11, 4],
    ]);
    expect(ringIconSize(2)).toBe(17);
    expect(ringIconSize(3)).toBe(15);
  });
});
