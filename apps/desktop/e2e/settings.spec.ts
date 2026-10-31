import { expect, test, type Page } from "@playwright/test";

/**
 * Setup form, provider picker, LAN consent, capability matrix and updater
 * surfaces. These are the paths that write state, so every failure branch is
 * exercised against the mock's scenario switches as well as the happy path.
 */
async function openSettings(page: Page, query = ""): Promise<void> {
  await page.goto(`/${query}`);
  await page.getByRole("button", { name: "设置", exact: true }).click();
  await expect(page.getByRole("heading", { name: "连接 AI 供应商账户" })).toBeVisible();
}

async function fillAccount(
  page: Page,
  fields: { name?: string; baseUrl?: string; apiKey?: string } = {},
): Promise<void> {
  if (fields.name !== undefined) await page.getByLabel("账户名称").fill(fields.name);
  if (fields.baseUrl !== undefined) await page.getByLabel("接口地址").fill(fields.baseUrl);
  if (fields.apiKey !== undefined) await page.getByLabel("API 密钥").fill(fields.apiKey);
}

test.describe("provider picker", () => {
  test("groups profiles by wire format and keeps platform variants in one capsule", async ({
    page,
  }) => {
    await openSettings(page);
    await expect(page.getByText("OpenAI 兼容格式")).toBeVisible();
    await expect(page.getByText("Anthropic 格式")).toBeVisible();
    await expect(page.getByText("Gemini 格式")).toBeVisible();
    await expect(page.getByText("自定义接入", { exact: true })).toBeVisible();

    // Anthropic's native protocol and its OpenAI-compatible variant share one
    // capsule; the variant list is behind the ellipsis.
    await expect(page.getByRole("button", { name: "Anthropic", exact: true })).toBeVisible();
    await expect(
      page.getByRole("button", { name: "Anthropic 的其他平台版本" }),
    ).toBeVisible();
  });

  test("selecting a built-in fills its fixed base URL and endpoint chips", async ({ page }) => {
    await openSettings(page);
    await page.getByRole("button", { name: "Anthropic", exact: true }).click();
    const baseUrl = page.getByLabel("接口地址");
    await expect(baseUrl).toHaveValue("https://api.anthropic.com/v1");
    await expect(baseUrl).toHaveAttribute("readonly", "");
    await expect(page.locator(".picker__chips")).toContainText("模型");
    // Corrected from the fabricated /v1/usage probe: Anthropic's official
    // surface here is models only, so no usage chip may appear.
    await expect(page.locator(".picker__chips")).not.toContainText("用量");
  });

  test("a variant selection updates the active profile and closes the menu", async ({
    page,
  }) => {
    await openSettings(page);
    // The trigger's accessible name follows the active variant, so match the
    // brand rather than one exact label.
    const more = page.getByRole("button", { name: /Anthropic.*其他平台版本/ });
    await more.click();
    await expect(more).toHaveAttribute("aria-expanded", "true");
    const option = page.getByRole("button", { name: /OpenAI 兼容/ });
    await option.click();
    await expect(more).toHaveAttribute("aria-expanded", "false");
    await expect(
      page.getByRole("button", { name: "Anthropic (OpenAI 兼容)", exact: true }),
    ).toHaveAttribute("aria-pressed", "true");
    await expect(page.getByLabel("接口地址")).toHaveValue("https://api.anthropic.com/v1");
  });

  test("Escape closes the variant menu and returns focus to its trigger", async ({ page }) => {
    await openSettings(page);
    const more = page.getByRole("button", { name: "Anthropic 的其他平台版本" });
    await more.click();
    await expect(page.getByRole("button", { name: /OpenAI 兼容/ })).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.getByRole("button", { name: /OpenAI 兼容/ })).toBeHidden();
    await expect(more).toBeFocused();
  });

  test("a gateway profile asks for its own host instead of a placeholder host", async ({
    page,
  }) => {
    await openSettings(page);
    await page.getByRole("button", { name: "New API", exact: true }).click();
    const gateway = page.getByLabel("网关地址");
    await expect(gateway).toHaveValue("");
    await expect(gateway).toHaveAttribute("placeholder", "http://localhost:4000/v1");
    await expect(page.getByText("填入你部署的网关地址")).toBeVisible();
  });
});

test.describe("custom wire format", () => {
  test("switching the format swaps the default host and hides probe endpoints", async ({
    page,
  }) => {
    await openSettings(page);
    await expect(page.getByLabel("额度接口（可选）")).toBeVisible();

    await page.getByRole("radio", { name: "Anthropic" }).click();
    await expect(page.getByLabel("接口地址")).toHaveValue("https://api.anthropic.com/v1");
    await expect(page.getByLabel("额度接口（可选）")).toHaveCount(0);
    await expect(page.getByText("由原生适配器探测")).toBeVisible();

    await page.getByRole("radio", { name: "Gemini" }).click();
    await expect(page.getByLabel("接口地址")).toHaveValue(
      "https://generativelanguage.googleapis.com/v1beta",
    );
    await expect(page.getByRole("radio", { name: "Gemini" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
  });
});

test.describe("form validation", () => {
  test("the API key field masks its value as a password", async ({ page }) => {
    await openSettings(page);
    await expect(page.getByLabel("API 密钥")).toHaveAttribute("type", "password");
  });

  test("an empty submit explains what is missing and focuses the name field", async ({
    page,
  }) => {
    await openSettings(page);
    await page.getByRole("button", { name: "连接账户" }).click();
    const alert = page.getByRole("alert");
    await expect(alert).toHaveText("请填写账户名称与接口地址。");
    await expect(page.getByLabel("账户名称")).toBeFocused();
  });

  test("a malformed URL is rejected before any IPC call", async ({ page }) => {
    await openSettings(page);
    await fillAccount(page, { name: "测试账户", baseUrl: "not-a-url", apiKey: "sk-test" });
    await page.getByRole("button", { name: "连接账户" }).click();
    await expect(page.getByRole("alert")).toHaveText(
      "接口地址必须是完整 URL，例如 https://api.example.com/v1。",
    );
    await expect(page.getByLabel("接口地址")).toBeFocused();
  });

  test("endpoint paths must start with a slash", async ({ page }) => {
    await openSettings(page);
    await fillAccount(page, {
      name: "测试账户",
      baseUrl: "https://api.example.com/v1",
      apiKey: "sk-test",
    });
    await page.getByLabel("用量接口（可选）").fill("v1/usage");
    await page.getByRole("button", { name: "连接账户" }).click();
    await expect(page.getByRole("alert")).toHaveText(
      "额度、用量、余额接口必须是以 / 开头的相对路径。",
    );
    await expect(page.getByLabel("用量接口（可选）")).toBeFocused();
  });

  test("a missing key is reported without submitting the secret field", async ({ page }) => {
    await openSettings(page);
    await fillAccount(page, { name: "测试账户", baseUrl: "https://api.example.com/v1" });
    await page.getByRole("button", { name: "连接账户" }).click();
    await expect(page.getByRole("alert")).toHaveText("请填写 API 密钥。");
    await expect(page.getByLabel("API 密钥")).toBeFocused();
  });
});

test.describe("account creation", () => {
  test("a successful connection resets the form and lands the account in the list", async ({
    page,
  }) => {
    await openSettings(page);
    await fillAccount(page, {
      name: "本地网关",
      baseUrl: "https://api.example.com/v1",
      apiKey: "sk-test",
    });
    await page.getByRole("button", { name: "连接账户" }).click();

    await expect(page.getByRole("status")).toContainText("账户已连接", {
      timeout: 15_000,
    });
    await expect(page.getByLabel("账户名称")).toHaveValue("");
    await expect(page.getByLabel("API 密钥")).toHaveValue("");

    await page.getByRole("button", { name: "概览", exact: true }).click();
    await expect(page.locator(".account")).toHaveCount(5, { timeout: 15_000 });
    await expect(page.locator(".account__label")).toContainText(["本地网关"]);
  });

  test("a rejected creation keeps every field for a corrected retry", async ({ page }) => {
    await openSettings(page, "?scenario=create-fail");
    await fillAccount(page, {
      name: "会被拒绝的账户",
      baseUrl: "https://api.example.com/v1",
      apiKey: "sk-test",
    });
    await page.getByRole("button", { name: "连接账户" }).click();
    await expect(page.getByRole("alert")).toHaveText(
      "账户连接失败，请检查接口地址与密钥后重试。",
      { timeout: 15_000 },
    );
    await expect(page.getByLabel("账户名称")).toHaveValue("会被拒绝的账户");
    await expect(page.getByLabel("API 密钥")).toHaveValue("sk-test");
  });

  test("a LAN target asks for explicit consent before creating anything", async ({ page }) => {
    await openSettings(page);
    await fillAccount(page, {
      name: "本机网关",
      baseUrl: "http://localhost:4000/v1",
      apiKey: "sk-test",
    });
    await page.getByRole("button", { name: "连接账户" }).click();

    const dialog = page.getByRole("alertdialog");
    await expect(dialog).toContainText("该地址位于局域网或本机");
    await expect(dialog).toHaveAttribute("aria-modal", "true");
    // Cancel leaves the form untouched and creates nothing.
    await dialog.getByRole("button", { name: "取消" }).click();
    await expect(page.getByRole("alertdialog")).toBeHidden();
    await expect(page.getByLabel("账户名称")).toHaveValue("本机网关");

    await page.getByRole("button", { name: "连接账户" }).click();
    await page
      .getByRole("alertdialog")
      .getByRole("button", { name: "我确认，继续" })
      .click();
    await expect(page.getByRole("status")).toContainText("账户已连接", {
      timeout: 15_000,
    });
  });

  test("classification failure fails closed with a visible error and no dialog", async ({
    page,
  }) => {
    await openSettings(page, "?scenario=lan-fail");
    await fillAccount(page, {
      name: "无法分类的网关",
      baseUrl: "http://localhost:4000/v1",
      apiKey: "sk-test",
    });
    await page.getByRole("button", { name: "连接账户" }).click();
    await expect(page.getByRole("alert")).toHaveText(
      "无法确认该地址是否属于局域网，已取消本次创建。请检查网络后重试。",
      { timeout: 15_000 },
    );
    await expect(page.getByRole("alertdialog")).toHaveCount(0);
  });

  test("community packs that fail validation are named, not silently dropped", async ({
    page,
  }) => {
    await openSettings(page, "?scenario=manifest-skipped");
    await expect(page.getByRole("status")).toContainText("部分社区包未能加载", {
      timeout: 15_000,
    });
    await expect(page.getByRole("status")).toContainText("broken.yaml");
    await expect(page.getByRole("status")).toContainText("old-pack.yaml");
  });
});

test.describe("capability matrix", () => {
  test("is collapsed by default, counts real rows, and marks planned providers", async ({
    page,
  }) => {
    await openSettings(page);
    const disclosure = page.locator(".matrix__disclosure");
    await expect(disclosure).not.toHaveAttribute("open", "");
    await disclosure.locator("summary").click();
    await expect(disclosure).toHaveAttribute("open", "");
    await expect(page.locator(".matrix__row")).toHaveCount(14);
    await expect(page.locator(".matrix__row--planned").first()).toContainText("规划中 · P0");
    // First row is OpenAI at its corrected honest level.
    await expect(page.locator(".matrix__row").first()).toContainText("L2 · 健康/模型");
  });
  test("a failed matrix read is reported instead of claiming zero providers", async ({
    page,
  }) => {
    await openSettings(page, "?scenario=matrix-fail");
    const disclosure = page.locator(".matrix__disclosure");
    await expect(disclosure.locator(".matrix__summary-count")).toHaveText("读取失败", {
      timeout: 15_000,
    });
    await disclosure.locator("summary").click();
    await expect(disclosure).toContainText("供应商能力矩阵暂时无法读取");
    await expect(disclosure.getByRole("button", { name: "重试" })).toBeVisible();
    await expect(page.locator(".matrix__row")).toHaveCount(0);
  });
});

test.describe("updater", () => {
  test("an up-to-date check renders where the button was pressed and dismisses", async ({
    page,
  }) => {
    await openSettings(page);
    const check = page.getByRole("button", { name: "检查更新" });
    await check.scrollIntoViewIfNeeded();
    await check.click();
    const banner = page.getByRole("status").filter({ hasText: "已是最新版本" });
    await expect(banner).toBeVisible();
    await expect(banner).toBeInViewport();
    await banner.getByRole("button", { name: "关闭此通知" }).click();
    await expect(banner).toBeHidden();
  });

  test("an available update offers download and reports the installed outcome", async ({
    page,
  }) => {
    await openSettings(page, "?scenario=update-available");
    const check = page.getByRole("button", { name: "检查更新" });
    await check.scrollIntoViewIfNeeded();
    await check.click();
    const banner = page.getByRole("status").filter({ hasText: "发现新版本 1.5.0" });
    await expect(banner).toBeVisible();
    await banner.getByRole("button", { name: "下载并安装" }).click();
    await expect(page.locator(".notice--accent")).toContainText("更新已安装", {
      timeout: 15_000,
    });
  });

  test("a failed check is reported as unavailable, never as up to date", async ({ page }) => {
    await openSettings(page, "?scenario=update-fail");
    const check = page.getByRole("button", { name: "检查更新" });
    await check.scrollIntoViewIfNeeded();
    await check.click();
    const notice = page.locator(".notice--warning").filter({ hasText: "当前无法检查或安装更新" });
    await expect(notice).toBeVisible({ timeout: 15_000 });
    await expect(page.getByText("已是最新版本")).toHaveCount(0);
  });

  test("a failed install keeps the app running and reports it on the shared notice", async ({
    page,
  }) => {
    await openSettings(page, "?scenario=update-available,install-fail");
    const check = page.getByRole("button", { name: "检查更新" });
    await check.scrollIntoViewIfNeeded();
    await check.click();
    const banner = page.getByRole("status").filter({ hasText: "发现新版本" });
    await expect(banner).toBeVisible();
    await banner.getByRole("button", { name: "下载并安装" }).click();
    await expect(page.locator(".notice--warning")).toContainText("当前无法检查或安装更新", {
      timeout: 15_000,
    });
  });
});
