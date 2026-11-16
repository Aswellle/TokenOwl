import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { RefreshSection } from "../features/dashboard/components/RefreshSection";
import type { DashboardBridge, RefreshSettings } from "../types";

function makeBridge(
  get: () => Promise<RefreshSettings>,
  set: (settings: RefreshSettings) => Promise<void>,
): DashboardBridge {
  return {
    getRefreshSettings: vi.fn(get),
    setRefreshSettings: vi.fn(set),
  } as unknown as DashboardBridge;
}

describe("refresh strategy settings", () => {
  it("loads the persisted cadence and saves a new one", async () => {
    const set = vi.fn(async () => {});
    const bridge = makeBridge(async () => ({ adaptive: false, cadenceMinutes: 0 }), set);
    render(<RefreshSection bridge={bridge} />);

    const select = await screen.findByRole("combobox", { name: "刷新间隔上限" });
    expect(select).toHaveValue("0");
    expect(screen.getByRole("option", { name: "按供应商默认" })).toBeInTheDocument();

    fireEvent.change(select, { target: { value: "5" } });
    await waitFor(() =>
      expect(set).toHaveBeenCalledWith({ adaptive: false, cadenceMinutes: 5 }),
    );
    expect(await screen.findByRole("status")).toHaveTextContent("刷新策略已保存");
  });

  it("persists the adaptive opt-in independently of the cadence", async () => {
    const set = vi.fn(async () => {});
    const bridge = makeBridge(async () => ({ adaptive: false, cadenceMinutes: 10 }), set);
    render(<RefreshSection bridge={bridge} />);

    const toggle = await screen.findByRole("checkbox", { name: /按消耗速度自动调整/ });
    fireEvent.click(toggle);
    await waitFor(() =>
      expect(set).toHaveBeenCalledWith({ adaptive: true, cadenceMinutes: 10 }),
    );
  });

  it("rolls back and explains a failed save", async () => {
    const bridge = makeBridge(
      async () => ({ adaptive: false, cadenceMinutes: 0 }),
      async () => {
        throw new Error("storage unavailable");
      },
    );
    render(<RefreshSection bridge={bridge} />);

    const select = await screen.findByRole("combobox", { name: "刷新间隔上限" });
    fireEvent.change(select, { target: { value: "15" } });
    expect(await screen.findByRole("alert")).toHaveTextContent("刷新策略保存失败");
    expect(select).toHaveValue("0");
  });

  it("reports a failed read instead of an endless skeleton", async () => {
    const bridge = makeBridge(async () => {
      throw new Error("offline");
    }, async () => {});
    render(<RefreshSection bridge={bridge} />);
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "刷新策略读取失败",
    );
  });
});
