import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { BackupSection } from "../features/dashboard/components/BackupSection";
import type {
  AccountImportPreview,
  AccountViewModel,
  DashboardBridge,
} from "../types";

const accounts: AccountViewModel[] = [
  {
    id: "a1",
    providerId: "deepseek:a1",
    providerName: "DeepSeek",
    label: "Work",
    status: "healthy",
    freshness: "fresh",
    snapshots: [],
  },
  {
    id: "a2",
    providerId: "openai:a2",
    providerName: "OpenAI",
    label: "Personal",
    status: "healthy",
    freshness: "fresh",
    snapshots: [],
  },
];

const preview: AccountImportPreview = {
  schemaVersion: 1,
  exportedAt: 1_800_000_000,
  entries: [
    {
      index: 0,
      providerId: "deepseek",
      providerName: "DeepSeek",
      label: "Work",
      baseUrl: "https://api.deepseek.com/v1",
      apiFormat: "openai_compatible",
      importable: true,
      reason: null,
      alreadyExists: true,
    },
  ],
};

function created(verified: boolean): AccountViewModel {
  return {
    id: "a3",
    providerId: "deepseek:a3",
    providerName: "DeepSeek",
    label: "Work",
    status: verified ? "healthy" : "error",
    freshness: "fresh",
    snapshots: [],
    diagnostics: { status: verified ? "healthy" : "failed", consecutiveFailures: 0 },
  };
}

function makeBridge(overrides: Partial<DashboardBridge> = {}): DashboardBridge {
  return {
    exportAccountConfig: vi.fn(async (ids?: string[]) => ({
      path: "/tmp/exports/tokenowl-accounts.json",
      accounts: ids?.length ?? accounts.length,
    })),
    previewAccountImport: vi.fn(async () => preview),
    createAccount: vi.fn(async () => created(true)),
    lanConsentBegin: vi.fn(async () => null),
    ...overrides,
  } as unknown as DashboardBridge;
}

describe("account configuration backup", () => {
  it("exports the selected accounts and reports the file", async () => {
    const bridge = makeBridge();
    render(<BackupSection bridge={bridge} accounts={accounts} onImported={() => {}} />);
    await waitFor(() =>
      expect(screen.getByRole("checkbox", { name: /Work/ })).toBeChecked(),
    );
    fireEvent.click(screen.getByRole("button", { name: "导出配置文件" }));

    await waitFor(() =>
      expect(bridge.exportAccountConfig).toHaveBeenCalledWith(["a1", "a2"]),
    );
    expect(await screen.findByRole("status")).toHaveTextContent(
      "已导出 2 个账户",
    );
  });

  it("previews a backup and creates each entry only after a new key", async () => {
    const onImported = vi.fn();
    const bridge = makeBridge();
    render(<BackupSection bridge={bridge} accounts={accounts} onImported={onImported} />);
    fireEvent.change(screen.getByLabelText("备份文件内容（JSON）"), {
      target: { value: '{"schemaVersion":1,"exportedAt":1,"accounts":[]}' },
    });
    fireEvent.click(screen.getByRole("button", { name: "校验并预览" }));

    expect(await screen.findByText(/备份格式版本 1/)).toBeInTheDocument();
    expect(screen.getByText("已有同名账户")).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText("Work 的 API 密钥"), {
      target: { value: "sk-test" },
    });
    fireEvent.click(screen.getByRole("button", { name: "输入密钥并添加" }));

    await waitFor(() =>
      expect(bridge.createAccount).toHaveBeenCalledWith(
        expect.objectContaining({ providerId: "deepseek", label: "Work", apiKey: "sk-test" }),
      ),
    );
    await waitFor(() => expect(onImported).toHaveBeenCalledTimes(1));
    expect(await screen.findByText("已添加，连接验证成功。")).toBeInTheDocument();
  });

  it("keeps the saved-but-unverified fact separate from success", async () => {
    const bridge = makeBridge({
      createAccount: vi.fn(async () => created(false)),
    });
    render(<BackupSection bridge={bridge} accounts={accounts} onImported={() => {}} />);
    fireEvent.change(screen.getByLabelText("备份文件内容（JSON）"), {
      target: { value: '{"schemaVersion":1,"exportedAt":1,"accounts":[]}' },
    });
    fireEvent.click(screen.getByRole("button", { name: "校验并预览" }));
    fireEvent.change(await screen.findByLabelText("Work 的 API 密钥"), {
      target: { value: "sk-test" },
    });
    fireEvent.click(screen.getByRole("button", { name: "输入密钥并添加" }));

    expect(await screen.findByText(/已添加，但暂时没有验证成功/)).toBeInTheDocument();
    expect(screen.queryByText("已添加，连接验证成功。")).not.toBeInTheDocument();
  });

  it("requires a key before calling the bridge and explains bad backups", async () => {
    const bridge = makeBridge({
      previewAccountImport: vi.fn(async () => {
        throw new Error("bad version");
      }),
    });
    render(<BackupSection bridge={bridge} accounts={accounts} onImported={() => {}} />);
    fireEvent.change(screen.getByLabelText("备份文件内容（JSON）"), {
      target: { value: "{}" },
    });
    fireEvent.click(screen.getByRole("button", { name: "校验并预览" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("不是有效的备份文件");
  });

  it("does not call create without a key", async () => {
    const bridge = makeBridge();
    render(<BackupSection bridge={bridge} accounts={accounts} onImported={() => {}} />);
    fireEvent.change(screen.getByLabelText("备份文件内容（JSON）"), {
      target: { value: "{}" },
    });
    fireEvent.click(screen.getByRole("button", { name: "校验并预览" }));
    fireEvent.click(await screen.findByRole("button", { name: "输入密钥并添加" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("请先输入这一条的 API 密钥");
    expect(bridge.createAccount).not.toHaveBeenCalled();
  });
});
