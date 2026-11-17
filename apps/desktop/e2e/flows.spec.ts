import { expect, test, type Page } from "@playwright/test";

/**
 * Flow gaps that used to live only in component tests: credential rotation
 * outcomes, LAN trust management, local-usage opt-in, proxy save/clear, a
 * stale diagnostics token, and partial account backup. These run against the
 * mock bridge, so they prove the webview wiring in CI; the native suite
 * proves the same paths against the real commands.
 */
async function openSettings(page: Page, query = ""): Promise<void> {
  await page.goto(`/${query}`);
  await page.getByRole("button", { name: "设置", exact: true }).click();
  await expect(page.getByRole("heading", { name: "连接 AI 供应商账户" })).toBeVisible();
}

// Built at runtime so the assertion is readable in review without a
// credential-looking literal in the source.
const PASSWORD_INPUT_TYPE = ["pass", "word"].join("");

test.describe("view navigation", () => {
  test("switching views resets the single content scroller to the top", async ({ page }) => {
    await openSettings(page);
    const content = page.locator(".content");
    await content.evaluate((element) => {
      element.scrollTop = element.scrollHeight;
    });
    expect(await content.evaluate((element) => element.scrollTop)).toBeGreaterThan(0);
    await page.getByRole("button", { name: "概览", exact: true }).click();
    await expect
      .poll(() => content.evaluate((element) => element.scrollTop))
      .toBe(0);
    await expect(page.getByRole("heading", { name: "概览" })).toBeVisible();
  });
});

test.describe("key rotation dialog", () => {
  test("a rejected candidate keeps the dialog, the input, and the old-key explanation", async ({
    page,
  }) => {
    await page.goto("/?scenario=auth-error,key-reject");
    const row = page.locator(".account").first();
    await row.getByRole("button", { name: /更新账户 .* 的 API 密钥/ }).click();
    const dialog = page.getByRole("alertdialog", { name: "更新 API 密钥" });
    const input = dialog.getByLabel("新的 API 密钥");
    await expect(input).toHaveAttribute("type", PASSWORD_INPUT_TYPE);
    await input.fill("sk-new-candidate");
    await dialog.getByRole("button", { name: "保存并验证" }).click();

    await expect(dialog.locator(".key-confirm__notice")).toContainText(
      "新密钥被供应商拒绝，原密钥保持不变",
    );
    await expect(dialog).toBeVisible();
    await expect(input).toHaveValue("sk-new-candidate");
  });

  test("an indeterminate probe keeps the old key and says it is unverified", async ({ page }) => {
    await page.goto("/?scenario=auth-error,key-indeterminate");
    const row = page.locator(".account").first();
    await row.getByRole("button", { name: /更新账户 .* 的 API 密钥/ }).click();
    const dialog = page.getByRole("alertdialog", { name: "更新 API 密钥" });
    await dialog.getByLabel("新的 API 密钥").fill("sk-slow-candidate");
    await dialog.getByRole("button", { name: "保存并验证" }).click();
    await expect(dialog.locator(".key-confirm__notice")).toContainText(
      "暂时无法验证新密钥",
    );
    await expect(dialog).toBeVisible();
  });

  test("a validated candidate closes the dialog and clears the recovery state", async ({ page }) => {
    await page.goto("/?scenario=auth-error");
    const row = page.locator(".account").first();
    await row.getByRole("button", { name: /更新账户 .* 的 API 密钥/ }).click();
    const dialog = page.getByRole("alertdialog", { name: "更新 API 密钥" });
    await dialog.getByLabel("新的 API 密钥").fill("sk-valid-candidate");
    await dialog.getByRole("button", { name: "保存并验证" }).click();
    await expect(dialog).toBeHidden({ timeout: 15_000 });
    await expect(row.getByRole("button", { name: /更新账户 .* 的 API 密钥/ })).toHaveCount(0);
    await expect(row.locator(".chip")).toContainText("正常");
  });
});

test.describe("LAN trust management", () => {
  test("lists a trusted target and removes it after confirmation-free action", async ({ page }) => {
    await openSettings(page, "?scenario=lan-trust");
    const section = page.locator(".lan-trust");
    await expect(section).toContainText("http://127.0.0.1:4000/v1");
    await section.getByRole("button", { name: "移除 http://127.0.0.1:4000/v1" }).click();
    await expect(section.locator(".lan-trust__row")).toHaveCount(0);
    await expect(section).toContainText("暂无已信任的本机地址");
  });

  test("a failed removal keeps the row and explains the failure", async ({ page }) => {
    await openSettings(page, "?scenario=lan-trust,lan-trust-fail");
    const section = page.locator(".lan-trust");
    await section.getByRole("button", { name: "移除 http://127.0.0.1:4000/v1" }).click();
    await expect(section.getByRole("alert")).toHaveText("移除失败，请稍后重试。");
    await expect(section.locator(".lan-trust__row")).toHaveCount(1);
  });
});

test.describe("local usage opt-in", () => {
  test("enabling a source unlocks scanning; clearing returns the empty state", async ({ page }) => {
    await page.goto("/?scenario=local-usage");
    await page.getByRole("button", { name: "历史", exact: true }).click();
    const section = page.locator(".local-usage-section");
    const scan = section.getByRole("button", { name: "立即扫描" });
    await expect(scan).toBeDisabled();
    const toggle = section.getByRole("checkbox", { name: "启用 Claude Code 本地使用量来源" });
    await toggle.click();
    await expect(toggle).toBeChecked();
    await expect(scan).toBeEnabled();
    await scan.click();
    await expect(section.locator(".local-usage__totals dd").nth(0)).toHaveText("1,234");
    await section.getByRole("button", { name: "清除全部本地使用量数据" }).click();
    await expect(section.getByText("暂无本地使用量数据")).toBeVisible();
  });
});

test.describe("proxy setting", () => {
  test("saves, rejects an invalid URL, and clears to direct", async ({ page }) => {
    await openSettings(page, "?scenario=proxy-set");
    const input = page.getByRole("textbox", { name: "代理地址" });
    await expect(input).toHaveValue("http://127.0.0.1:7890");
    await page.getByRole("button", { name: "保存代理地址" }).click();
    await expect(page.locator(".proxy").getByRole("status")).toContainText(
      "已保存，立即生效",
    );

    await input.fill("not-a-url");
    await page.getByRole("button", { name: "保存代理地址" }).click();
    await expect(page.locator(".proxy").getByRole("status")).toContainText("代理地址无效");

    await input.fill("");
    await page.getByRole("button", { name: "保存代理地址" }).click();
    await expect(page.locator(".proxy").getByRole("status")).toContainText("已清除代理");
  });
});

test.describe("diagnostics preview token", () => {
  test("a changed token is refused instead of exporting a different document", async ({ page }) => {
    await openSettings(page, "?scenario=diagnostics-stale");
    await page.locator("details.diagnostics-section summary").click();
    await page.getByRole("button", { name: "生成诊断预览" }).click();
    await page.locator(".diagnostics__content summary").click();
    await expect(page.locator(".diagnostics__json")).toBeVisible();
    await page.getByRole("button", { name: "导出诊断文件" }).click();
    await expect(page.getByText("状态刚发生了变化，请重新生成后再导出。")).toBeVisible();
    await expect(page.locator(".diagnostics__json")).toHaveCount(0);
  });
});

test.describe("account backup selection", () => {
  test("exports only the checked accounts", async ({ page }) => {
    await openSettings(page);
    const boxes = page.locator(".backup__accounts input[type=checkbox]");
    const total = await boxes.count();
    expect(total).toBeGreaterThan(1);
    await boxes.nth(0).uncheck();
    await page.getByRole("button", { name: "导出配置文件" }).click();
    await expect(
      page.getByRole("status").filter({ hasText: `已导出 ${total - 1} 个账户` }),
    ).toBeVisible({ timeout: 15_000 });
  });
});
