import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { ProviderView } from "../features/dashboard/components/ProviderView";
import type { AccountViewModel, DashboardBridge } from "../types";

vi.mock("@lobehub/icons", () => ({
  ProviderIcon: () => null,
}));

const account: AccountViewModel = {
  id: "a1",
  providerId: "deepseek:a1",
  providerName: "DeepSeek",
  label: "Work",
  status: "healthy",
  freshness: "fresh",
  capabilityLevel: "L3",
  snapshots: [],
  lastUpdatedAt: 1_800_000_000,
};

function makeBridge(): DashboardBridge {
  return {
    usageWindows: vi.fn(async () => ({
      hours2: { tokens: 0, hasData: false },
      hours24: { tokens: 0, hasData: false },
      days30: { tokens: 0, hasData: false },
    })),
    historySummary: vi.fn(async () => ({
      days: [],
      totalTokens: 0,
      activeDays: 0,
      rangeDays: 30,
    })),
    providerMatrix: vi.fn(async () => []),
    testAccountEndpoints: vi.fn(async () => ({
      accountId: "a1",
      providerId: "deepseek",
      baseHost: "api.deepseek.com",
      customEndpoint: false,
      testedAt: 1_800_000_000,
      probes: [
        { kind: "models", outcome: "ok", httpStatus: 200, latencyMs: 42 },
        { kind: "quota", outcome: "auth", httpStatus: 401, latencyMs: 55 },
      ],
    })),
  } as unknown as DashboardBridge;
}

describe("provider endpoint diagnostics", () => {
  it("runs the test and shows one classified row per endpoint", async () => {
    const bridge = makeBridge();
    render(
      <ProviderView
        account={account}
        now={1_800_000_000_000}
        refreshing={false}
        bridge={bridge}
        onBack={() => {}}
        onRefresh={() => {}}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "测试端点" }));

    await waitFor(() =>
      expect(bridge.testAccountEndpoints).toHaveBeenCalledWith("a1"),
    );
    expect(await screen.findByText("可访问")).toBeInTheDocument();
    expect(screen.getByText("认证失败")).toBeInTheDocument();
    expect(screen.getByText(/HTTP 401/)).toBeInTheDocument();
    expect(screen.getByText(/api\.deepseek\.com/)).toBeInTheDocument();
  });

  it("explains a command failure without claiming the endpoints are fine", async () => {
    const bridge = makeBridge();
    bridge.testAccountEndpoints = vi.fn(async () => {
      throw new Error("ipc down");
    });
    render(
      <ProviderView
        account={account}
        now={1_800_000_000_000}
        refreshing={false}
        bridge={bridge}
        onBack={() => {}}
        onRefresh={() => {}}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "测试端点" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("端点测试未能执行");
  });
});
