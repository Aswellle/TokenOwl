import { describe, expect, it } from "vitest";
import { overallHealth, pressure, quotaRatio, ratioTone } from "../lib/format";
import type { AccountViewModel, QuotaViewModel } from "../types";

function quota(overrides: Partial<QuotaViewModel>): QuotaViewModel {
  return {
    id: "q1",
    metricName: "quota",
    unit: "token",
    ...overrides,
  };
}

function account(status: AccountViewModel["status"], quotas?: QuotaViewModel[]): AccountViewModel {
  return {
    id: "a1",
    providerId: "generic.openai_compatible",
    providerName: "Test",
    label: "Test",
    status,
    freshness: "fresh",
    snapshots: quotas ?? [],
  };
}

describe("ratioTone", () => {
  it("treats strictly-below-10% as critical", () => {
    expect(ratioTone(0.099)).toBe("critical");
    expect(ratioTone(0)).toBe("critical");
  });

  it("keeps exactly-10% and the band under 25% at attention", () => {
    expect(ratioTone(0.1)).toBe("attention");
    expect(ratioTone(0.2)).toBe("attention");
    expect(ratioTone(0.249)).toBe("attention");
  });

  it("treats 25% and above as healthy", () => {
    expect(ratioTone(0.25)).toBe("healthy");
    expect(ratioTone(0.9)).toBe("healthy");
  });

  it("returns null for unmeasurable pressure", () => {
    expect(ratioTone(null)).toBeNull();
  });
});

describe("quotaRatio", () => {
  it("prefers remaining/limit and derives from used when remaining is missing", () => {
    expect(quotaRatio(quota({ remaining: 20, limit: 100, used: 80 }))).toBe(0.2);
    expect(quotaRatio(quota({ used: 80, limit: 100 }))).toBeCloseTo(0.2);
  });

  it("returns null when no usable pair exists", () => {
    expect(quotaRatio(quota({}))).toBeNull();
    expect(quotaRatio(quota({ limit: 0, remaining: 5 }))).toBeNull();
  });
});

describe("pressure", () => {
  it("takes the worst ratio across snapshots", () => {
    const ratios = pressure([
      quota({ remaining: 60, limit: 100 }),
      quota({ remaining: 8, limit: 100 }),
    ]);
    expect(ratios).toBe(0.08);
  });

  it("returns null without any measurable snapshot", () => {
    expect(pressure([quota({})])).toBeNull();
    expect(pressure([])).toBeNull();
  });
});

describe("overallHealth", () => {
  it("propagates critical and attention before stale", () => {
    expect(overallHealth([account("healthy"), account("critical")], "ready")).toBe("critical");
    expect(overallHealth([account("healthy"), account("attention")], "ready")).toBe("attention");
    expect(overallHealth([account("attention"), account("stale")], "ready")).toBe("attention");
    expect(overallHealth([account("error")], "ready")).toBe("error");
  });

  it("falls back to healthy only when no account signals a problem", () => {
    expect(overallHealth([account("healthy")], "ready")).toBe("healthy");
  });
});
