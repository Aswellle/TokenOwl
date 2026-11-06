import { describe, it, expect, vi } from "vitest";
vi.mock("@lobehub/icons", () => ({
  ProviderIcon: () => null,
}));
import { render, screen, fireEvent } from "@testing-library/react";
import { AccountRow } from "../features/dashboard/components/AccountRow";
import type { AccountViewModel, QuotaViewModel } from "../types";

const mockQuota: QuotaViewModel = {
  id: "q1",
  metricName: "5h quota",
  unit: "token",
  used: 250,
  limit: 1000,
  remaining: 750,
  window: { type: "rolling" },
  observedAt: "2026-10-10T12:00:00Z",
  source: "official_api",
  freshness: "fresh",
  status: "available",
};

const mockAccount: AccountViewModel = {
  id: "acc-1",
  providerId: "openai",
  providerName: "OpenAI",
  label: "Test Account",
  status: "healthy",
  freshness: "fresh",
  snapshots: [mockQuota],
  source: "official_api",
  lastUpdatedAt: "2026-10-10T12:00:00Z",
};

describe("AccountRow", () => {
  it("renders account label and provider name", () => {
    render(
      <AccountRow
        account={mockAccount}
        index={0}
        now={Date.now()}
        refreshing={false}
        deleting={false}
        updatingKey={false}
        onUpdateKey={vi.fn(async () => true)}
        onOpen={() => {}}
        onRefresh={() => {}}
        onDelete={() => {}}
      />,
    );
    expect(screen.getByText("Test Account")).toBeInTheDocument();
    expect(screen.getByText("OpenAI")).toBeInTheDocument();
  });

  it("renders quota metrics when snapshots exist", () => {
    render(
      <AccountRow
        account={mockAccount}
        index={0}
        now={Date.now()}
        refreshing={false}
        deleting={false}
        updatingKey={false}
        onUpdateKey={vi.fn(async () => true)}
        onOpen={() => {}}
        onRefresh={() => {}}
        onDelete={() => {}}
      />,
    );
    expect(screen.getByText("5h quota")).toBeInTheDocument();
  });

  it("opens the provider view from the card face and refreshes from its own button", () => {
    const onOpen = vi.fn();
    const onRefresh = vi.fn();
    render(
      <AccountRow
        account={mockAccount}
        index={0}
        now={Date.now()}
        refreshing={false}
        deleting={false}
        updatingKey={false}
        onUpdateKey={vi.fn(async () => true)}
        onOpen={onOpen}
        onRefresh={onRefresh}
        onDelete={() => {}}
      />,
    );
    // The card's face (icon + name) is the navigation affordance; the refresh
    // control stays a separate target.
    fireEvent.click(screen.getByTitle(/查看供应商/));
    expect(onOpen).toHaveBeenCalledOnce();
    expect(onRefresh).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: /刷新账户/ }));
    expect(onRefresh).toHaveBeenCalledOnce();
  });

  it("deletes only after the inline confirm, and cancel aborts", () => {
    const onDelete = vi.fn();
    render(
      <AccountRow
        account={mockAccount}
        index={0}
        now={Date.now()}
        refreshing={false}
        deleting={false}
        updatingKey={false}
        onUpdateKey={vi.fn(async () => true)}
        onOpen={() => {}}
        onRefresh={() => {}}
        onDelete={onDelete}
      />,
    );
    // The removal button opens an inline confirm instead of deleting
    // outright; a stray click must not lose the account.
    const remove = screen.getByRole("button", { name: /移除账户/ });
    fireEvent.click(remove);
    expect(onDelete).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: /取消/ }));
    expect(onDelete).not.toHaveBeenCalled();

    fireEvent.click(remove);
    fireEvent.click(screen.getByRole("button", { name: /确认移除/ }));
    expect(onDelete).toHaveBeenCalledOnce();
  });

  it("keeps the card face navigable while the mutation buttons are disabled", () => {
    render(
      <AccountRow
        account={mockAccount}
        index={0}
        now={Date.now()}
        refreshing={true}
        deleting={false}
        updatingKey={false}
        onUpdateKey={vi.fn(async () => true)}
        onOpen={() => {}}
        onRefresh={() => {}}
        onDelete={() => {}}
      />,
    );
    expect(screen.getByRole("button", { name: /刷新账户/ })).toBeDisabled();
    expect(screen.getByRole("button", { name: /移除账户/ })).toBeDisabled();
    expect(screen.getByTitle(/查看供应商/)).toBeEnabled();
  });

  it("shows provider icon", () => {
    render(
      <AccountRow
        account={mockAccount}
        index={0}
        now={Date.now()}
        refreshing={false}
        deleting={false}
        updatingKey={false}
        onUpdateKey={vi.fn(async () => true)}
        onOpen={() => {}}
        onRefresh={() => {}}
        onDelete={() => {}}
      />,
    );
    const icon = document.querySelector(".provider-icon");
    expect(icon).toBeInTheDocument();
  });

  it("explains a first refresh failure instead of claiming old values exist", () => {
    render(
      <AccountRow
        account={{
          ...mockAccount,
          status: "error",
          freshness: "unknown",
          snapshots: [],
        }}
        index={0}
        now={Date.now()}
        refreshing={false}
        deleting={false}
        updatingKey={false}
        onUpdateKey={vi.fn(async () => true)}
        onOpen={() => {}}
        onRefresh={() => {}}
        onDelete={() => {}}
      />,
    );
    expect(screen.getByText(/首次刷新失败/)).toBeInTheDocument();
    expect(screen.queryByText("刷新失败，下方显示的是最近一次成功的数据。")).not.toBeInTheDocument();
    // The generic "click refresh" empty hint would repeat the failure notice.
    expect(screen.queryByText("点击刷新以获取该账户的第一个快照。")).not.toBeInTheDocument();
  });

  it("names the missing quota capability instead of reading it as a fault", () => {
    render(
      <AccountRow
        account={{
          ...mockAccount,
          status: "unavailable",
          freshness: "unknown",
          snapshots: [],
          capabilityLevel: "L2",
        }}
        index={0}
        now={Date.now()}
        refreshing={false}
        deleting={false}
        updatingKey={false}
        onUpdateKey={vi.fn(async () => true)}
        onOpen={() => {}}
        onRefresh={() => {}}
        onDelete={() => {}}
      />,
    );
    expect(screen.getByText(/此供应商不提供密钥级额度查询/)).toBeInTheDocument();
    // The generic wording would read like the connection is broken.
    expect(screen.queryByText(/接口没有返回可识别的额度字段/)).not.toBeInTheDocument();
  });

  it("keeps the generic unavailable wording for capability-less custom accounts", () => {
    render(
      <AccountRow
        account={{
          ...mockAccount,
          status: "unavailable",
          freshness: "unknown",
          snapshots: [],
        }}
        index={0}
        now={Date.now()}
        refreshing={false}
        deleting={false}
        updatingKey={false}
        onUpdateKey={vi.fn(async () => true)}
        onOpen={() => {}}
        onRefresh={() => {}}
        onDelete={() => {}}
      />,
    );
    expect(screen.getByText(/接口没有返回可识别的额度字段/)).toBeInTheDocument();
  });

  it("offers the key update for credential failures and names it as the fix", () => {
    render(
      <AccountRow
        account={{
          ...mockAccount,
          status: "error",
          freshness: "unknown",
          snapshots: [],
          errorCode: "AuthExpired",
          errorMessage: "provider rejected the credential",
        }}
        index={0}
        now={Date.now()}
        refreshing={false}
        deleting={false}
        updatingKey={false}
        onUpdateKey={vi.fn(async () => true)}
        onOpen={() => {}}
        onRefresh={() => {}}
        onDelete={() => {}}
      />,
    );
    expect(screen.getByRole("button", { name: /更新账户 Test Account 的 API 密钥/ })).toBeInTheDocument();
  });

  it("withholds the key update for rate limiting and says why", () => {
    render(
      <AccountRow
        account={{
          ...mockAccount,
          status: "error",
          freshness: "unknown",
          snapshots: [],
          errorCode: "RateLimited",
          errorMessage: "provider answered 429",
        }}
        index={0}
        now={Date.now()}
        refreshing={false}
        deleting={false}
        updatingKey={false}
        onUpdateKey={vi.fn(async () => true)}
        onOpen={() => {}}
        onRefresh={() => {}}
        onDelete={() => {}}
      />,
    );
    expect(screen.queryByRole("button", { name: /更新账户 Test Account 的 API 密钥/ })).not.toBeInTheDocument();
    expect(screen.getByText(/供应商正在限流/)).toBeInTheDocument();
  });

  it("offers no recovery action for failures a key cannot fix", () => {
    render(
      <AccountRow
        account={{
          ...mockAccount,
          status: "error",
          freshness: "unknown",
          snapshots: [],
          errorCode: "NetworkUnavailable",
          errorMessage: "provider request timed out",
        }}
        index={0}
        now={Date.now()}
        refreshing={false}
        deleting={false}
        updatingKey={false}
        onUpdateKey={vi.fn(async () => true)}
        onOpen={() => {}}
        onRefresh={() => {}}
        onDelete={() => {}}
      />,
    );
    expect(screen.queryByRole("button", { name: /更新账户 Test Account 的 API 密钥/ })).not.toBeInTheDocument();
    expect(screen.queryByText(/供应商正在限流/)).not.toBeInTheDocument();
  });
});
