import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { LanTrustSection } from "../features/dashboard/components/LanTrustSection";
import type { DashboardBridge } from "../types";

function makeBridge(trusted: string[], overrides: Partial<DashboardBridge> = {}): DashboardBridge {
  return {
    lanTrustList: vi.fn(async () => [...trusted]),
    lanTrustRemove: vi.fn(async () => undefined),
    ...overrides,
  } as unknown as DashboardBridge;
}

describe("LanTrustSection", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("lists trusted targets with a revoke action per row", async () => {
    const bridge = makeBridge(["http://127.0.0.1:4000/v1", "http://192.168.1.9:9000"]);
    render(<LanTrustSection bridge={bridge} />);

    expect(await screen.findByText("http://127.0.0.1:4000/v1")).toBeInTheDocument();
    expect(screen.getByText("http://192.168.1.9:9000")).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: /移除 http/ })).toHaveLength(2);
  });

  it("explains the empty state instead of hiding the section", async () => {
    render(<LanTrustSection bridge={makeBridge([])} />);
    expect(await screen.findByText(/暂无已信任的本机地址/)).toBeInTheDocument();
  });

  it("drops the row after a successful removal", async () => {
    const bridge = makeBridge(["http://127.0.0.1:4000/v1"]);
    render(<LanTrustSection bridge={bridge} />);
    await screen.findByText("http://127.0.0.1:4000/v1");

    fireEvent.click(screen.getByRole("button", { name: /移除 http/ }));

    await waitFor(() =>
      expect(bridge.lanTrustRemove).toHaveBeenCalledWith("http://127.0.0.1:4000/v1"),
    );
    await waitFor(() =>
      expect(screen.queryByText("http://127.0.0.1:4000/v1")).not.toBeInTheDocument(),
    );
  });

  it("keeps the row and reports the failure when removal rejects", async () => {
    const bridge = makeBridge(["http://127.0.0.1:4000/v1"], {
      lanTrustRemove: vi.fn(async () => {
        throw new Error("store busy");
      }),
    });
    render(<LanTrustSection bridge={bridge} />);
    await screen.findByText("http://127.0.0.1:4000/v1");

    fireEvent.click(screen.getByRole("button", { name: /移除 http/ }));

    await waitFor(() => expect(screen.getByRole("alert")).toBeInTheDocument());
    expect(screen.getByText("http://127.0.0.1:4000/v1")).toBeInTheDocument();
  });
});
