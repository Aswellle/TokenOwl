import { expect, test } from "./fixtures";
import fs from "node:fs/promises";
import path from "node:path";
import { clearTestCredentials, createNativeDirs, launchNativeApp } from "./native-app";
import { accountRow, openOverview } from "./helpers";

const KEY = ["sk", "native", "old"].join("-");

function manifest(): string {
  return `schemaVersion: 1
id: acme-native
version: 1
name: Acme Native
category: community
region: global
auth: [api_key]
baseUrl: https://acme.example/v1
customBaseUrl: true
apiKeyHeader: Authorization
endpoints:
  models: /models
  usage: /usage
  reset: /reset
mapping:
  usage:
    value: data.used
    unit: token
    role: used
  reset:
    value: data.reset_at
    timestampFormat: unix_seconds
capability:
  level: L3
  accountTypes: [api]
  quotaUnits: [token]
  targetWindows: [provider-defined]
  priority: P2
`;
}

/**
 * A community pack is data, not a plugin: this test installs one into the
 * real app-data directory, creates an account from it, proves its declared
 * endpoints are the ones contacted, and checks that a malformed pack is
 * named in the skip list instead of disappearing.
 * exercised against the same pack.
 */
test("a community pack loads, connects, and its invalid sibling is reported", async ({
  provider,
}) => {
  const dirs = await createNativeDirs();
  const packDirectory = path.join(dirs.appData, "community-providers");
  await fs.mkdir(packDirectory, { recursive: true });
  await fs.writeFile(path.join(packDirectory, "acme-native.yaml"), manifest());
  await fs.writeFile(
    path.join(packDirectory, "bad-pack.yaml"),
    manifest().replace("apiKeyHeader: Authorization", "script: alert(1)"),
  );

  provider.acceptedKeys.add(KEY);
  const app = await launchNativeApp({ dirs });
  try {
    const page = app.page;
    await page.getByRole("button", { name: "设置", exact: true }).click();

    // The invalid pack is named, not silently dropped.
    const skipped = page.getByRole("status").filter({ hasText: "部分社区提供的配置未能加载" });
    await expect(skipped).toContainText("bad-pack.yaml");
    await expect(skipped).not.toContainText("acme-native.yaml");
    const packs = page.locator("details.community-packs");
    await packs.locator("summary").click();
    await expect(packs).toContainText("acme-native.yaml");
    await expect(packs).toContainText("Acme Native");

    // The picker profile is selectable and its manifest owns the URL.
    await page.getByRole("button", { name: "Acme Native", exact: true }).click();
    // A community gateway carries a placeholder host; the typed URL is the
    // real target and the pack's endpoints still apply.
    await expect(page.getByLabel("网关地址")).toHaveValue("");
    await page.getByLabel("网关地址").fill(provider.baseUrl);
    await page.getByLabel("账户名称").fill("社区账户");
    await page.getByLabel("API 密钥").fill(KEY);
    await page.getByRole("button", { name: "连接账户" }).click();
    const dialog = page.getByRole("alertdialog");
    await expect(dialog).toBeVisible();
    await dialog.getByRole("button", { name: "我确认，继续" }).click();
    await expect(page.locator(".form__success")).toContainText("账户已连接", {
      timeout: 20_000,
    });

    // The refresh contacted the pack's mapped usage endpoint.
    expect(provider.requests.map((request) => request.path)).toContain("/v1/usage");
    await openOverview(page);
    const row = accountRow(page, "社区账户");
    await expect(row).toContainText("Acme Native");
    await expect(row).toContainText("123");
    await expect(row).toContainText("用量");

    // Endpoint diagnostics cover all three declared endpoints.
    await row.locator(".account__identity").click();
    await page.getByRole("button", { name: "测试端点" }).click();
    const endpointRows = page.locator(".endpoint-test__row");
    await expect(endpointRows).toHaveCount(3, { timeout: 15_000 });
    for (let index = 0; index < 3; index += 1) {
      await expect(endpointRows.nth(index)).toContainText("可访问");
    }

  } finally {
    await app.close();
    await clearTestCredentials(dirs.credentialService);
  }
});
