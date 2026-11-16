import { expect, test } from "./fixtures";
import {
  clearTestCredentials,
  createNativeDirs,
  launchNativeApp,
  type NativeApp,
} from "./native-app";
import { appDataContains, openNotifications, openSettings } from "./helpers";

const PROXY_PASSWORD = ["proxy", "pass", "native"].join("-");

/**
 * Preferences that must survive a process restart: refresh cadence/adaptive,
 * dock collapse, notification muting, and the proxy URL. The proxy password
 * is read back masked and never appears in SQLite or the app-data tree.
 */
test("refresh, dock, notification, and masked proxy preferences survive a restart", async () => {
  const dirs = await createNativeDirs();
  let app: NativeApp = await launchNativeApp({ dirs });
  try {
    let page = app.page;

    await openSettings(page);
    await page.getByLabel("刷新间隔上限").selectOption("10");
    await expect(
      page.locator(".refresh-settings").getByRole("status"),
    ).toContainText("刷新策略已保存");
    const adaptive = page.getByRole("checkbox", { name: /按消耗速度自动调整/ });
    await adaptive.click();
    await expect(adaptive).toBeChecked();

    const proxy = page.getByRole("textbox", { name: "网络代理" });
    await proxy.fill(`http://user:${PROXY_PASSWORD}@127.0.0.1:7890`);
    await page.getByRole("button", { name: "保存代理设置" }).click();
    await expect(
      page.locator(".proxy").getByRole("status"),
    ).toContainText("已保存，重启应用后生效");

    // A remount reads the stored value back through proxy_get, which masks
    // the password segment; the real value never re-enters the webview.
    await page.getByRole("button", { name: "概览", exact: true }).click();
    await openSettings(page);
    const proxyInput = page.getByRole("textbox", { name: "网络代理" });
    await expect(proxyInput).toHaveValue(/^http:\/\/user:/, { timeout: 10_000 });
    const masked = await proxyInput.inputValue();
    expect(masked).toMatch(/^http:\/\/user:.+@127\.0\.0\.1:7890$/);
    expect(masked).not.toContain(PROXY_PASSWORD);
    expect(await appDataContains(app, PROXY_PASSWORD)).toBe(false);

    // Mute alerts and collapse the dock, then restart the process.
    await openNotifications(page);
    const settings = page.locator("details.notif-settings");
    await settings.locator("summary").click();
    const enabled = settings.getByRole("checkbox", { name: /启用通知/ });
    await enabled.uncheck();
    await expect(settings.locator("summary")).toContainText("已停用");
    await expect(settings.getByRole("status")).toContainText("已保存");
    await openOverviewDock(page);
    await page.getByRole("button", { name: "收起控制台" }).click();
    await expect(page.locator(".dock-wrap")).toHaveClass(/dock-wrap--collapsed/);

    await app.close({ keepDirs: true });
    await new Promise((resolve) => setTimeout(resolve, 500));
    app = await launchNativeApp({ dirs });
    page = app.page;
    await expect(page.locator(".dock-wrap")).toHaveClass(/dock-wrap--collapsed/, {
      timeout: 15_000,
    });
    await expect(page.getByRole("button", { name: "显示控制台" })).toHaveAttribute(
      "aria-expanded",
      "false",
    );

    // The dock is inert while collapsed; expand it to navigate.
    await page.getByRole("button", { name: "显示控制台" }).click();
    await openSettings(page);
    await expect(page.getByLabel("刷新间隔上限")).toHaveValue("10");
    await expect(page.getByRole("checkbox", { name: /按消耗速度自动调整/ })).toBeChecked();
    await expect(page.getByRole("textbox", { name: "网络代理" })).toHaveValue(masked);

    await openNotifications(page);
    await expect(page.locator("details.notif-settings summary")).toContainText("已停用");
  } finally {
    await app.close();
    await clearTestCredentials(dirs.credentialService);
  }
});

/** The overview dock item is available regardless of the active view. */
async function openOverviewDock(page: import("@playwright/test").Page): Promise<void> {
  await page.getByRole("button", { name: "概览", exact: true }).click();
  await expect(page.getByRole("heading", { name: "概览" })).toBeVisible();
}
