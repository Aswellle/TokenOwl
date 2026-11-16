import { expect, test } from "./fixtures";
import { testCredentialTargets } from "./native-app";
import {
  accountRow,
  createCustomAccount,
  exportCount,
  latestExport,
  openOverview,
} from "./helpers";

// Built at runtime so no credential-shaped literal enters the source.
const IMPORTED_KEY = ["sk", "native", "old"].join("-");

/**
 * Secret-free backup round trip and the reviewed diagnostics bundle, both
 * through the real commands. The backup is re-imported only after a key
 * re-entry, and the diagnostics export is refused once the preview is stale.
 */
test("account backup round-trips without secrets and diagnostics export requires a fresh preview", async ({
  app,
  provider,
}) => {
  const page = app.page;
  const service = app.dirs.credentialService;

  await page.getByRole("button", { name: "设置", exact: true }).click();
  await createCustomAccount(page, {
    label: "备份账户",
    baseUrl: provider.baseUrl,
    apiKey: IMPORTED_KEY,
  });

  await expect
    .poll(async () => (await testCredentialTargets(service)).length)
    .toBe(1);

  // Export only the selected account.
  const checkbox = page.getByRole("checkbox", { name: /备份账户/ });
  await expect(checkbox).toBeChecked();
  await page.getByRole("button", { name: "导出配置" }).click();
  await expect(page.getByRole("status").filter({ hasText: "已导出 1 个账户" })).toBeVisible({
    timeout: 15_000,
  });
  const backup = await latestExport(app, "accounts", ".json");
  const backupDoc = JSON.parse(backup.text) as {
    schemaVersion: number;
    exportedAt: number;
    accounts: Array<Record<string, unknown>>;
  };
  expect(backupDoc.schemaVersion).toBe(1);
  expect(backupDoc.accounts).toHaveLength(1);
  expect(backupDoc.accounts[0]).toMatchObject({
    providerId: "generic.openai_compatible",
    label: "备份账户",
    baseUrl: provider.baseUrl,
    apiFormat: "openai_compatible",
    quotaEndpoint: "/quota",
  });
  expect(backupDoc.accounts[0]).not.toHaveProperty("credentialRef");
  expect(backupDoc.accounts[0]).not.toHaveProperty("accountId");
  expect(backup.text).not.toContain(IMPORTED_KEY);
  expect(backup.text.toLowerCase()).not.toContain("credential");

  // Previewing while the same account exists warns without creating anything.
  const payload = page.getByLabel("备份 JSON");
  await payload.fill(backup.text);
  await page.getByRole("button", { name: "校验备份" }).click();
  await expect(page.getByText(/schema v1 · 1 个账户/)).toBeVisible();
  await expect(page.getByText("已存在同名账户")).toBeVisible();

  // Delete the account: the config row and its Credential Manager entry both
  // go away, while the reviewed backup remains on disk.
  await openOverview(page);
  const row = accountRow(page, "备份账户");
  await row.getByRole("button", { name: "移除账户 备份账户" }).click();
  await page.getByRole("button", { name: "确认移除" }).click();
  await expect(page.locator(".account")).toHaveCount(0, { timeout: 15_000 });
  await expect
    .poll(async () => (await testCredentialTargets(service)).length)
    .toBe(0);

  // Re-import: preview no longer flags a duplicate, and the row requires a
  // fresh key before the normal create path runs.
  await page.getByRole("button", { name: "设置", exact: true }).click();
  await page.getByLabel("备份 JSON").fill(backup.text);
  await page.getByRole("button", { name: "校验备份" }).click();
  await expect(page.getByText(/schema v1 · 1 个账户/)).toBeVisible();
  await expect(page.getByText("已存在同名账户")).toHaveCount(0);
  await page.getByLabel("备份账户 的 API Key").fill(IMPORTED_KEY);
  await page.getByRole("button", { name: "添加并验证" }).click();
  const lan = page.getByRole("alertdialog", { name: "导入的账户指向本地网络" });
  await expect(lan).toBeVisible();
  await lan.getByRole("button", { name: "确认并继续" }).click();
  await expect(page.getByText("已添加，首次刷新验证成功。")).toBeVisible({
    timeout: 20_000,
  });
  expect(await testCredentialTargets(service)).toHaveLength(1);

  await openOverview(page);
  await expect(accountRow(page, "备份账户")).toContainText("58%");

  // Diagnostics: review, export, then make the document change and prove the
  // stale token is refused instead of writing a different bundle.
  await page.getByRole("button", { name: "设置", exact: true }).click();
  await page.locator("details.diagnostics-section summary").click();
  await page.getByRole("button", { name: "生成诊断预览" }).click();
  const preview = page.locator(".diagnostics__json");
  await expect(preview).toBeVisible({ timeout: 15_000 });
  const bundle = JSON.parse(await preview.innerText()) as {
    schemaVersion: number;
    app: { version: string; os: string; arch: string };
    storage: { schemaVersion: number; accountCount: number };
    settings: { cadenceMinutes: number };
    accounts: Array<Record<string, unknown>>;
  };
  expect(bundle.schemaVersion).toBe(1);
  expect(bundle.app.version).toMatch(/^\d+\.\d+\.\d+/);
  expect(bundle.storage.accountCount).toBe(1);
  expect(bundle.accounts[0]).toMatchObject({
    providerId: "generic.openai_compatible",
    customEndpoint: true,
    baseHost: null,
  });
  expect(bundle.accounts[0].endpointKinds).toContain("quota");
  const text = await preview.innerText();
  for (const forbidden of [IMPORTED_KEY, "备份账户", "127.0.0.1", service, "credentialRef"]) {
    expect(text).not.toContain(forbidden);
  }

  await page.getByRole("button", { name: "导出诊断包" }).click();
  await expect(page.getByRole("status").filter({ hasText: "已导出诊断包" })).toBeVisible({
    timeout: 15_000,
  });
  const diagnostics = await latestExport(app, "diagnostics", ".json");
  const exported = JSON.parse(diagnostics.text) as typeof bundle;
  expect(exported.accounts).toEqual(bundle.accounts);
  expect(exported.storage.accountCount).toBe(1);
  expect(diagnostics.text).not.toContain(IMPORTED_KEY);
  expect(diagnostics.text).not.toContain("127.0.0.1");
  expect(await exportCount(app, "diagnostics")).toBe(1);

  // Changing a setting changes the bundle the token covers.
  await page.getByLabel("刷新间隔上限").selectOption("10");
  await expect(page.locator(".refresh-settings").getByRole("status")).toContainText(
    "刷新策略已保存",
  );
  await page.getByRole("button", { name: "导出诊断包" }).click();
  await expect(page.getByText("诊断内容已变化，请重新生成预览后再导出。")).toBeVisible();
  await expect(preview).toHaveCount(0);
  expect(await exportCount(app, "diagnostics")).toBe(1);
});
