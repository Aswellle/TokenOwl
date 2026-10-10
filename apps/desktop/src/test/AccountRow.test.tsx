import { describe, it, expect, vi } from "vitest";
vi.mock("@lobehub/icons", () => ({
  ProviderIcon: () => null,
}));
import { render, screen, fireEvent } from "@testing-library/react";
import { AccountRow } from "../features/dashboard/components/AccountRow";
import type { AccountViewModel, QuotaViewModel } from "../../../types";

const mockQuota: QuotaViewModel = {
  id: "q1",
  metricName: "5h quota",
  unit: "token",
  used: 250,
  limit: 1000,
  remaining: 750,
  window: { type: "rolling", durationSeconds: 18000 },
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
        onRefresh={() => {}}
        onDelete={() => {}}
      />,
    );
    expect(screen.getByText("5h quota")).toBeInTheDocument();
  });

  it("calls onRefresh when refresh button clicked", () => {
    const onRefresh = vi.fn();
    render(
      <AccountRow
        account={mockAccount}
        index={0}
        now={Date.now()}
        refreshing={false}
        deleting={false}
        onRefresh={onRefresh}
        onDelete={() => {}}
      />,
    );
    const buttons = screen.getAllByRole("button");
    fireEvent.click(buttons[0]);
    expect(onRefresh).toHaveBeenCalledOnce();
  });

  it("calls onDelete when delete button clicked", () => {
    const onDelete = vi.fn();
    render(
      <AccountRow
        account={mockAccount}
        index={0}
        now={Date.now()}
        refreshing={false}
        deleting={false}
        onRefresh={() => {}}
        onDelete={onDelete}
      />,
    );
    const buttons = screen.getAllByRole("button");
    fireEvent.click(buttons[1]);
    expect(onDelete).toHaveBeenCalledOnce();
  });

  it("disables buttons when refreshing", () => {
    render(
      <AccountRow
        account={mockAccount}
        index={0}
        now={Date.now()}
        refreshing={true}
        deleting={false}
        onRefresh={() => {}}
        onDelete={() => {}}
      />,
    );
    const buttons = screen.getAllByRole("button");
    buttons.forEach((btn) => expect(btn).toBeDisabled());
  });

  it("shows provider icon", () => {
    render(
      <AccountRow
        account={mockAccount}
        index={0}
        now={Date.now()}
        refreshing={false}
        deleting={false}
        onRefresh={() => {}}
        onDelete={() => {}}
      />,
    );
    const icon = document.querySelector(".provider-icon");
    expect(icon).toBeInTheDocument();
  });
});
