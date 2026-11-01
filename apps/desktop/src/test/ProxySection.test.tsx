import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { ProxySection } from "../features/dashboard/components/ProxySection";
import type { DashboardBridge } from "../types";

function makeBridge(overrides: Partial<DashboardBridge> = {}): DashboardBridge {
  return {
    getProxy: vi.fn(async () => null),
    lanTrustList: vi.fn(async () => []),
    lanTrustAdd: vi.fn(async () => undefined),
    lanTrustRemove: vi.fn(async () => undefined),
    setProxy: vi.fn(async () => undefined),
    ...overrides,
  } as unknown as DashboardBridge;
}

describe("ProxySection", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("pre-fills the stored proxy URL and saves a trimmed value", async () => {
    const bridge = makeBridge({ getProxy: vi.fn(async () => "http://127.0.0.1:7890") });
    render(<ProxySection bridge={bridge} />);
    const input = await screen.findByRole("textbox", { name: "网络代理" });
    expect(input).toHaveValue("http://127.0.0.1:7890");

    fireEvent.change(input, { target: { value: "  http://10.0.0.8:3128  " } });
    fireEvent.click(screen.getByRole("button", { name: "保存代理设置" }));

    await waitFor(() =>
      expect(bridge.setProxy).toHaveBeenCalledWith("http://10.0.0.8:3128"),
    );
    await waitFor(() => expect(screen.getByText(/重启应用后生效/)).toBeInTheDocument());
  });

  it("sends null to clear when the field is empty", async () => {
    const bridge = makeBridge({ getProxy: vi.fn(async () => "http://127.0.0.1:7890") });
    render(<ProxySection bridge={bridge} />);
    const input = await screen.findByRole("textbox", { name: "网络代理" });
    fireEvent.change(input, { target: { value: "   " } });
    fireEvent.click(screen.getByRole("button", { name: "保存代理设置" }));

    await waitFor(() => expect(bridge.setProxy).toHaveBeenCalledWith(null));
    await waitFor(() => expect(screen.getByText(/恢复直连/)).toBeInTheDocument());
  });

  it("keeps the field editable and reports the failure when saving rejects", async () => {
    const bridge = makeBridge({
      setProxy: vi.fn(async () => {
        throw new Error("invalid");
      }),
    });
    render(<ProxySection bridge={bridge} />);
    const input = await screen.findByRole("textbox", { name: "网络代理" });
    fireEvent.change(input, { target: { value: "http://127.0.0.1:7890" } });
    fireEvent.click(screen.getByRole("button", { name: "保存代理设置" }));

    await waitFor(() => expect(screen.getByText(/代理地址无效/)).toBeInTheDocument());
    expect(input).toBeEnabled();
  });
});
