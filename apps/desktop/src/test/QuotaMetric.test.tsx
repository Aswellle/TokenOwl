import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { QuotaMetric } from "../features/dashboard/components/QuotaMetric";
import type { QuotaViewModel } from "../types";

/**
 * Unit-aware quota cards: every quota KIND the app distinguishes (rolling /
 * fixed / subscription windows, credits, currency balances, lifetime token
 * totals, percent readings) must render in its own vocabulary — and a
 * windowless figure must never grow a fake reset countdown or ratio bar.
 */
function baseQuota(extra: Partial<QuotaViewModel>): QuotaViewModel {
  return {
    id: "q-1",
    metricName: "测试指标",
    unit: "token",
    observedAt: Date.now(),
    source: "OFFICIAL_API",
    freshness: "fresh",
    status: "available",
    ...extra,
  };
}

describe("QuotaMetric quota kinds", () => {
  const now = Date.now();

  it("labels a rolling window with its span tag", () => {
    render(
      <QuotaMetric
        now={now}
        quota={baseQuota({
          metricName: "5 小时用量窗口",
          used: 100,
          limit: 800,
          remaining: 700,
          window: {
            type: "rolling",
            startsAt: new Date(now - 2 * 3_600_000).toISOString(),
            endsAt: new Date(now + 3 * 3_600_000).toISOString(),
            resetAt: now + 3 * 3_600_000,
          },
        })}
      />,
    );
    expect(screen.getByText(/滚动窗口 · 5h/)).toBeInTheDocument();
    expect(screen.getByText(/后重置/)).toBeInTheDocument();
  });

  it("labels a fixed weekly window and a subscription period differently", () => {
    render(
      <QuotaMetric
        now={now}
        quota={baseQuota({
          metricName: "每周额度",
          unit: "token",
          used: 70,
          limit: 100,
          remaining: 30,
          window: { type: "fixed", resetAt: now + 5 * 86_400_000 },
        })}
      />,
    );
    expect(screen.getByText(/固定窗口 · 7d/)).toBeInTheDocument();
  });

  it("shows a currency balance without a window, bar, or reset", () => {
    const { container } = render(
      <QuotaMetric
        now={now}
        quota={baseQuota({
          metricName: "现金余额",
          unit: "currency",
          currency: "CNY",
          used: null,
          limit: null,
          remaining: 3.0,
          window: null,
        })}
      />,
    );
    expect(screen.getByText("CNY 3")).toBeInTheDocument();
    // No ratio exists (no limit) and no window exists: neither may be faked.
    expect(container.querySelector(".bar")).toBeNull();
    expect(container.querySelector(".quota__reset")).toBeNull();
    expect(container.querySelector(".quota__window")).toBeNull();
  });

  it("compacts large token counts instead of printing every digit", () => {
    render(
      <QuotaMetric
        now={now}
        quota={baseQuota({
          metricName: "输入 Token 用量",
          unit: "token",
          used: 18_234_567,
          limit: null,
          remaining: null,
        })}
      />,
    );
    expect(screen.getByText("1,823.46 万")).toBeInTheDocument();
  });

  it("reads a percent metric as a share", () => {
    render(
      <QuotaMetric
        now={now}
        quota={baseQuota({ metricName: "缓存命中", unit: "percent", used: 82.4 })}
      />,
    );
    expect(screen.getByText("82.4%")).toBeInTheDocument();
  });

  it("keeps provider-named balance splits visible with their own labels", () => {
    render(
      <QuotaMetric
        now={now}
        quota={baseQuota({
          metricName: "赠送余额",
          unit: "currency",
          currency: "CNY",
          remaining: 0.88,
          window: null,
        })}
      />,
    );
    expect(screen.getByText("赠送余额")).toBeInTheDocument();
    expect(screen.getByText("CNY 0.88")).toBeInTheDocument();
  });
});
