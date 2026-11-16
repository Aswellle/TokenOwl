import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { DiagnosticsSection } from "../features/dashboard/components/DiagnosticsSection";
import type { DashboardBridge } from "../types";

function makeBridge(overrides: Partial<DashboardBridge> = {}): DashboardBridge {
  return {
    diagnosticsPreview: vi.fn(async () => ({
      token: "abc123",
      generatedAt: 1_800_000_000,
      json: '{"schemaVersion":1,"accounts":[]}',
    })),
    exportDiagnostics: vi.fn(async () => ({
      path: "/tmp/exports/tokenowl-diagnostics.json",
      bytes: 34,
    })),
    ...overrides,
  } as unknown as DashboardBridge;
}

describe("local diagnostics bundle", () => {
  it("previews the exact document and exports it with the preview token", async () => {
    const bridge = makeBridge();
    render(<DiagnosticsSection bridge={bridge} />);
    fireEvent.click(screen.getByRole("button", { name: "生成诊断预览" }));

    expect(await screen.findByText(/"schemaVersion":1/)).toBeInTheDocument();
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "导出诊断包" }),
      ).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByRole("button", { name: "导出诊断包" }));

    await waitFor(() =>
      expect(bridge.exportDiagnostics).toHaveBeenCalledWith("abc123"),
    );
    expect(await screen.findByRole("status")).toHaveTextContent(
      "已导出诊断包",
    );
  });

  it("drops the preview when the token went stale instead of writing another document", async () => {
    const bridge = makeBridge({
      exportDiagnostics: vi.fn(async () => {
        throw new Error("diagnostics changed");
      }),
    });
    render(<DiagnosticsSection bridge={bridge} />);
    fireEvent.click(screen.getByRole("button", { name: "生成诊断预览" }));
    await screen.findByText(/"schemaVersion":1/);
    fireEvent.click(screen.getByRole("button", { name: "导出诊断包" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("诊断内容已变化");
    expect(screen.queryByText(/"schemaVersion":1/)).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "导出诊断包" }),
    ).not.toBeInTheDocument();
  });

  it("reports preview generation failures", async () => {
    const bridge = makeBridge({
      diagnosticsPreview: vi.fn(async () => {
        throw new Error("offline");
      }),
    });
    render(<DiagnosticsSection bridge={bridge} />);
    fireEvent.click(screen.getByRole("button", { name: "生成诊断预览" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("诊断预览生成失败");
  });
});
