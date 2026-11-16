import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { ManifestPlayground } from "../features/dashboard/components/ManifestPlayground";
import type { DashboardBridge, ManifestPlaygroundViewModel } from "../types";

function makeBridge(
  run: () => Promise<ManifestPlaygroundViewModel>,
): DashboardBridge {
  return { manifestPlayground: vi.fn(run) } as unknown as DashboardBridge;
}

describe("manifest playground", () => {
  it("shows the normalized metrics returned for a sample body", async () => {
    const bridge = makeBridge(async () => ({
      valid: true,
      errors: [],
      warnings: [],
      schemaVersion: 1,
      id: "acme-relay",
      capabilityLevel: "L3",
      declaredEndpoints: ["usage"],
      metrics: [
        {
          metricId: "usage",
          metricName: "usage",
          unit: "token",
          used: 42.5,
          status: "partial",
        },
      ],
      reset: null,
    }));
    render(<ManifestPlayground bridge={bridge} />);
    fireEvent.click(screen.getByRole("button", { name: "本地解析" }));

    expect(await screen.findByText(/清单通过结构校验/)).toBeInTheDocument();
    expect(screen.getByText(/used 42.5/)).toBeInTheDocument();
    await waitFor(() =>
      expect(bridge.manifestPlayground).toHaveBeenCalledWith(
        expect.objectContaining({ endpoint: "usage" }),
      ),
    );
  });

  it("renders manifest errors without pretending the sample parsed", async () => {
    const bridge = makeBridge(async () => ({
      valid: false,
      errors: ["manifest.script is not a recognized manifest field"],
      warnings: [],
      declaredEndpoints: [],
      metrics: [],
      reset: null,
    }));
    render(<ManifestPlayground bridge={bridge} />);
    fireEvent.click(screen.getByRole("button", { name: "本地解析" }));

    expect(await screen.findByText(/清单未通过校验/)).toBeInTheDocument();
    expect(screen.getByText(/manifest.script/)).toBeInTheDocument();
  });

  it("reports an IPC failure instead of leaving the previous result", async () => {
    const bridge = makeBridge(async () => {
      throw new Error("bridge down");
    });
    render(<ManifestPlayground bridge={bridge} />);
    fireEvent.click(screen.getByRole("button", { name: "本地解析" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("本地解析失败");
  });
});
