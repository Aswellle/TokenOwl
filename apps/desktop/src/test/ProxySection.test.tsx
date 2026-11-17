import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { ProxySection } from "../features/dashboard/components/ProxySection";
import type { DashboardBridge } from "../types";

function makeBridge(overrides: Partial<DashboardBridge> = {}): DashboardBridge {
  return {
    getProxy: vi.fn(async () => null),
    setProxy: vi.fn(async () => undefined),
    getProxyRouting: vi.fn(async () => ({ domestic: "direct", international: "system" })),
    setProxyRouting: vi.fn(async (routing) => routing),
    lanTrustList: vi.fn(async () => []),
    lanTrustAdd: vi.fn(async () => undefined),
    lanTrustRemove: vi.fn(async () => undefined),
    getDockCollapsed: vi.fn(async () => false),
    setDockCollapsed: vi.fn(async () => undefined),
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
    const input = await screen.findByRole("textbox", { name: "代理地址" });
    expect(input).toHaveValue("http://127.0.0.1:7890");

    fireEvent.change(input, { target: { value: "  http://10.0.0.8:3128  " } });
    fireEvent.click(screen.getByRole("button", { name: "保存代理地址" }));

    await waitFor(() =>
      expect(bridge.setProxy).toHaveBeenCalledWith("http://10.0.0.8:3128"),
    );
    // Saving applies immediately now; the copy must not promise a restart.
    // The group intro also says "立即生效", so match the status sentence.
    await waitFor(() =>
      expect(screen.getByText("已保存，立即生效。")).toBeInTheDocument(),
    );
  });

  it("sends null to clear when the field is empty", async () => {
    const bridge = makeBridge({ getProxy: vi.fn(async () => "http://127.0.0.1:7890") });
    render(<ProxySection bridge={bridge} />);
    const input = await screen.findByRole("textbox", { name: "代理地址" });
    fireEvent.change(input, { target: { value: "   " } });
    fireEvent.click(screen.getByRole("button", { name: "保存代理地址" }));

    await waitFor(() => expect(bridge.setProxy).toHaveBeenCalledWith(null));
    await waitFor(() => expect(screen.getByText(/已清除代理地址/)).toBeInTheDocument());
  });

  it("keeps the field editable and names the fix when the backend rejects the URL", async () => {
    const bridge = makeBridge({
      setProxy: vi.fn(async () => {
        throw {
          code: "InvalidInput",
          message: "invalid account input: proxy URL is not a valid absolute URL",
        };
      }),
    });
    render(<ProxySection bridge={bridge} />);
    const input = await screen.findByRole("textbox", { name: "代理地址" });
    fireEvent.change(input, { target: { value: "http://127.0.0.1:7890" } });
    fireEvent.click(screen.getByRole("button", { name: "保存代理地址" }));

    await waitFor(() => expect(screen.getByText(/代理地址无效/)).toBeInTheDocument());
    expect(input).toBeEnabled();
  });

  it("blames the credential store, not the URL, when the store rejects the save", async () => {
    const bridge = makeBridge({
      setProxy: vi.fn(async () => {
        throw { code: "Credential", message: "credential error: unavailable" };
      }),
    });
    render(<ProxySection bridge={bridge} />);
    const input = await screen.findByRole("textbox", { name: "代理地址" });
    fireEvent.change(input, { target: { value: "http://127.0.0.1:7890" } });
    fireEvent.click(screen.getByRole("button", { name: "保存代理地址" }));

    await waitFor(() => expect(screen.getByText(/凭据库/)).toBeInTheDocument());
    expect(input).toBeEnabled();
  });

  it("loads the routed policy and saves a changed class immediately", async () => {
    const setProxyRouting = vi.fn(async (routing) => routing);
    const bridge = makeBridge({
      getProxy: vi.fn(async () => "http://127.0.0.1:7890"),
      setProxyRouting,
    });
    render(<ProxySection bridge={bridge} />);

    const domestic = await screen.findByRole("combobox", { name: "国内供应商" });
    const international = screen.getByRole("combobox", { name: "国际供应商" });
    await waitFor(() => expect(domestic).toHaveValue("direct"));
    expect(international).toHaveValue("system");

    fireEvent.change(international, { target: { value: "direct" } });

    await waitFor(() =>
      expect(setProxyRouting).toHaveBeenCalledWith({
        domestic: "direct",
        international: "direct",
      }),
    );
  });

  it("rolls the routing select back when the save fails", async () => {
    const bridge = makeBridge({
      getProxy: vi.fn(async () => "http://127.0.0.1:7890"),
      setProxyRouting: vi.fn(async () => {
        throw new Error("rejected");
      }),
    });
    render(<ProxySection bridge={bridge} />);

    const international = await screen.findByRole("combobox", { name: "国际供应商" });
    await waitFor(() => expect(international).toHaveValue("system"));
    fireEvent.change(international, { target: { value: "direct" } });

    expect(await screen.findByText(/分流规则保存失败/)).toBeInTheDocument();
    expect(international).toHaveValue("system");
  });
});
