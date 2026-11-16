import { expect, type Locator, type Page } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import type { NativeApp } from "./native-app";

export async function openSettings(page: Page): Promise<void> {
  await page.getByRole("button", { name: "设置", exact: true }).click();
  await expect(page.getByRole("heading", { name: "连接 AI 供应商账户" })).toBeVisible();
}

export async function openOverview(page: Page): Promise<void> {
  await page.getByRole("button", { name: "概览", exact: true }).click();
  await expect(page.getByRole("heading", { name: "概览" })).toBeVisible();
}

export async function openHistory(page: Page): Promise<void> {
  await page.getByRole("button", { name: "历史", exact: true }).click();
  await expect(page.getByRole("heading", { name: "历史" })).toBeVisible();
}

export async function openNotifications(page: Page): Promise<void> {
  await page.getByRole("button", { name: "通知", exact: true }).click();
  await expect(page.getByRole("heading", { name: "通知" })).toBeVisible();
}

export function accountRow(page: Page, label: string): Locator {
  return page.locator(".account").filter({ hasText: label });
}

export interface CreateAccountOptions {
  label: string;
  baseUrl: string;
  apiKey: string;
  quotaEndpoint?: string | null;
  confirmLan?: boolean;
}

/** Fills the custom OpenAI-compatible form and waits for the real first refresh. */
export async function createCustomAccount(
  page: Page,
  options: CreateAccountOptions,
): Promise<void> {
  await page.getByLabel("账户名称").fill(options.label);
  await page.getByLabel("接口地址").fill(options.baseUrl);
  if (options.quotaEndpoint !== null) {
    await page.getByLabel("额度接口（可选）").fill(options.quotaEndpoint ?? "/quota");
  }
  await page.getByLabel("API 密钥").fill(options.apiKey);
  await page.getByRole("button", { name: "连接账户" }).click();

  if (options.confirmLan !== false) {
    const dialog = page.getByRole("alertdialog");
    await expect(dialog).toBeVisible({ timeout: 5_000 });
    await dialog.getByRole("button", { name: "我确认，继续" }).click();
  }
  await expect(page.locator(".form__success")).toContainText("账户已连接", {
    timeout: 20_000,
  });
}

/** Newest export of one kind in the isolated app-data exports directory. */
export async function latestExport(
  app: NativeApp,
  prefix: "accounts" | "diagnostics" | "history",
  extension: ".json" | ".csv",
): Promise<{ path: string; text: string }> {
  const directory = path.join(app.dirs.appData, "exports");
  const entries = await fs.readdir(directory);
  const matching = entries
    .filter((entry) => entry.startsWith(`tokenowl-${prefix}-`) && entry.endsWith(extension))
    .sort();
  expect(matching.length).toBeGreaterThan(0);
  const file = path.join(directory, matching[matching.length - 1]);
  return { path: file, text: await fs.readFile(file, "utf8") };
}

export async function exportCount(
  app: NativeApp,
  prefix: "accounts" | "diagnostics" | "history",
): Promise<number> {
  const directory = path.join(app.dirs.appData, "exports");
  const entries = await fs.readdir(directory).catch(() => [] as string[]);
  return entries.filter((entry) => entry.startsWith(`tokenowl-${prefix}-`)).length;
}

export async function appDataContains(app: NativeApp, needle: string): Promise<boolean> {
  const entries = await fs.readdir(app.dirs.appData, { recursive: true });
  for (const entry of entries) {
    const full = path.join(app.dirs.appData, String(entry));
    const stats = await fs.stat(full).catch(() => null);
    if (!stats?.isFile()) continue;
    const bytes = await fs.readFile(full);
    if (bytes.includes(Buffer.from(needle))) return true;
  }
  return false;
}
