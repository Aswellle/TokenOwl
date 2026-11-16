import { expect, test } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { FixtureProvider } from "./fixture-provider";
import { launchNativeApp, type NativeApp } from "./native-app";

/**
 * Native integration smoke: the real Tauri process, real Rust IPC, real
 * SQLite/keyring, and a loopback provider. No mock bridge and no Vite dev
 * server participate in these paths.
 */
test.describe("native shell", () => {
  let app: NativeApp | null = null;

  test.afterEach(async () => {
    await app?.close();
    app = null;
  });

  test("first run renders the real bridge with no preview data", async () => {
    app = await launchNativeApp();
    const page = app.page;

    expect(await page.evaluate(() => document.body.dataset.preview ?? null)).toBeNull();
    await expect(page.getByRole("heading", { name: "概览" })).toBeVisible();
    await expect(page.getByText("尚未连接", { exact: true }).first()).toBeVisible();

    // The real command surface is reachable (the browser mock never mounts).
    const accounts = await page.evaluate(async () => {
      const internals = (window as unknown as {
        __TAURI_INTERNALS__?: { invoke?: unknown };
      }).__TAURI_INTERNALS__;
      return typeof internals?.invoke;
    });
    expect(accounts).toBe("function");

    await page.getByRole("button", { name: "设置", exact: true }).click();
    await expect(
      page.getByRole("heading", { name: "连接 AI 供应商账户" }),
    ).toBeVisible();

    // Native manifests are embedded, not the smaller browser mock table.
    await expect(page.locator(".picker__group").first()).toBeVisible();
    const profiles = await page.locator(".picker__group").count();
    // 31 manifests collapse into 27 brand groups (Kimi/Zhipu/Gemini/
    // Anthropic each hide a second platform variant behind the ellipsis).
    expect(profiles).toBe(27);

    const capabilities = page.locator("details.matrix__disclosure");
    await capabilities.locator("summary").click();
    await expect(page.locator(".matrix__row").first()).toBeVisible();
    const rows = await page.locator(".matrix__row").count();
    expect(rows).toBe(32);
    await expect(page.locator(".matrix__row").first()).toContainText("契约测试覆盖");
  });

  test("a debug build reports the updater as unavailable instead of heartbeating releases", async () => {
    app = await launchNativeApp();
    const page = app.page;

    await page.getByRole("button", { name: "设置", exact: true }).click();
    await expect(page.getByRole("heading", { name: "连接 AI 供应商账户" })).toBeVisible();
    await page.getByRole("button", { name: "检查更新" }).click();
    // The updater plugin is compiled out of debug builds: the command fails
    // closed instead of silently probing the production release endpoint on
    // every test launch.
    await expect(
      page.getByText("当前无法检查或安装更新，本机数据未受影响。"),
    ).toBeVisible({ timeout: 15_000 });
    await expect(page.getByText(/已是最新版本/)).toHaveCount(0);
  });

  test("creates a custom account, refreshes real quota, tests its endpoint, and keeps the key out of storage", async () => {
    const provider = new FixtureProvider();
    await provider.start();
    provider.acceptedKeys.add("sk-native-old");
    app = await launchNativeApp();
    const page = app.page;

    try {
      await page.getByRole("button", { name: "设置", exact: true }).click();
      await page.getByLabel("账户名称").fill("本机夹具");
      await page.getByLabel("接口地址").fill(provider.baseUrl);
      await page.getByLabel("额度接口（可选）").fill("/quota");
      await page.getByLabel("API 密钥").fill("sk-native-old");
      await page.getByRole("button", { name: "连接账户" }).click();

      // loopback is a LAN target: the real Rust classifier raises the consent
      // gate and the token is minted by the backend, not by the mock bridge.
      const dialog = page.getByRole("alertdialog");
      await expect(dialog).toContainText("该地址位于局域网或本机");
      await dialog.getByRole("button", { name: "我确认，继续" }).click();
      await expect(page.locator(".form__success")).toContainText("账户已连接", {
        timeout: 20_000,
      });
      await expect(page.getByLabel("API 密钥")).toHaveValue("");

      // The key never rides in the webview once the command completes.
      const leakedInDom = await page.evaluate(() => {
        const storage = JSON.stringify({
          local: { ...localStorage },
          session: { ...sessionStorage },
        });
        return (
          document.documentElement.outerHTML.includes("sk-native-old") ||
          document.body.innerText.includes("sk-native-old") ||
          storage.includes("sk-native-old")
        );
      });
      expect(leakedInDom).toBe(false);

      await page.getByRole("button", { name: "概览", exact: true }).click();
      const row = page.locator(".account").filter({ hasText: "本机夹具" });
      await expect(row).toBeVisible({ timeout: 20_000 });
      await expect(row).toContainText("58%");
      await expect(row).toContainText("18%");
      await expect(row).toContainText("58 万");
      await expect(row).toContainText("42 万");
      expect(provider.quotaRequests().some((request) => request.apiKey === "sk-native-old")).toBe(
        true,
      );

      // Provider detail + endpoint test hit the fixture through the real
      // adapter and report each declared endpoint once.
      await row.locator(".account__identity").click();
      await expect(page.getByRole("region", { name: "本机夹具" })).toBeVisible();
      await page.getByRole("button", { name: "测试端点" }).click();
      const endpoint = page.locator(".endpoint-test");
      await expect(endpoint).toBeVisible({ timeout: 15_000 });
      await expect(endpoint).toContainText("自定义端点（地址已隐藏）");
      await expect(endpoint.locator(".endpoint-test__row")).toHaveCount(1);
      await expect(endpoint.locator(".endpoint-test__row")).toContainText("可访问");
      await expect(endpoint.locator(".endpoint-test__row")).toContainText("HTTP 200");

      const dataFiles = await fs.readdir(app.dirs.appData, { recursive: true });
      for (const file of dataFiles) {
        const full = path.join(app.dirs.appData, String(file));
        const stats = await fs.stat(full).catch(() => null);
        if (!stats?.isFile()) continue;
        const bytes = await fs.readFile(full);
        expect(bytes.includes(Buffer.from("sk-native-old"))).toBe(false);
      }
    } finally {
      await provider.stop();
    }
  });
});
