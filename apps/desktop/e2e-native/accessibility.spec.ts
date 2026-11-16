import { expect, test } from "./fixtures";
import type { Page } from "@playwright/test";
import { runAudit, type Audit } from "./audit";
import { resizeMainWindow } from "./native-app";
import {
  createCustomAccount,
  openHistory,
  openNotifications,
  openOverview,
  openSettings,
} from "./helpers";

const KEY = ["sk", "native", "old"].join("-");

function expectClean(audit: Audit, view: string): void {
  console.log(`a11y ${view}`, JSON.stringify(audit, null, 2));
  expect(audit.mainLandmarks, `${view}: one main landmark`).toBe(1);
  expect(audit.navLandmarks, `${view}: named navigation`).toBeGreaterThanOrEqual(1);
  expect(audit.headings[0], `${view}: starts at h1`).toMatch(/^H1:/);
  expect(audit.namelessButtons, `${view}: button names`).toEqual([]);
  expect(audit.unlabelledControls, `${view}: control labels`).toEqual([]);
  expect(audit.duplicateIds, `${view}: duplicate ids`).toEqual([]);
  expect(audit.smallTargets, `${view}: target sizes`).toEqual([]);
  expect(audit.contrastViolations, `${view}: contrast`).toEqual([]);
}

async function focusRing(page: Page): Promise<void> {
  const dock = page.getByRole("button", { name: "历史", exact: true });
  await dock.focus();
  const outline = await dock.evaluate((element) => {
    const style = getComputedStyle(element);
    return { style: style.outlineStyle, width: style.outlineWidth };
  });
  expect(outline.style).not.toBe("none");
  expect(Number.parseFloat(outline.width)).toBeGreaterThanOrEqual(1);
}

/**
 * Native accessibility/usability audit. The browser e2e suite covers the
 * mock preview; this runs the same structural checks against WebView2 with
 * real data on every populated view, plus a Win32 resize to the minimum
 * window and a keyboard dialog trap.
 */
test("native UI exposes landmarks, names, focus, readable contrast, and min-width reflow", async ({
  app,
  provider,
}) => {
  const page = app.page;
  expectClean(await runAudit(page), "overview-empty");
  await focusRing(page);

  // The LAN dialog traps Tab, starts on the safe action, and restores focus.
  await openSettings(page);
  await page.getByLabel("账户名称").fill("键盘账户");
  await page.getByLabel("接口地址").fill("http://127.0.0.1:9/v1");
  await page.getByLabel("API 密钥").fill(KEY);
  await page.getByRole("button", { name: "连接账户" }).click();
  const dialog = page.getByRole("alertdialog");
  await expect(dialog).toBeVisible();
  const cancel = dialog.getByRole("button", { name: "取消" });
  const confirm = dialog.getByRole("button", { name: "我确认，继续" });
  await expect(cancel).toBeFocused();
  await page.keyboard.press("Tab");
  await expect(confirm).toBeFocused();
  await page.keyboard.press("Tab");
  await expect(cancel).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(dialog).toBeHidden();
  await expect(page.getByRole("button", { name: "连接账户" })).toBeFocused();
  await page.getByRole("button", { name: "取消" }).click();

  // Minimum window: the document must not scroll horizontally and the dock
  // stays inside the viewport. Then the same at a wide size.
  const dpr = await page.evaluate(() => window.devicePixelRatio);
  const minimum = await resizeMainWindow(app.pid, 560 * dpr, 480 * dpr);
  expect(minimum?.ok).toBe(true);
  await page.waitForTimeout(400);
  const narrow = await page.evaluate(() => ({
    width: window.innerWidth,
    height: window.innerHeight,
    scrollWidth: document.documentElement.scrollWidth,
    dockRight: document.querySelector(".dock-wrap")?.getBoundingClientRect().right ?? 0,
  }));
  expect(narrow.scrollWidth).toBeLessThanOrEqual(narrow.width + 1);
  expect(narrow.dockRight).toBeLessThanOrEqual(narrow.width + 1);
  await page.screenshot({ path: test.info().outputPath("narrow-560x480.png") });

  const wide = await resizeMainWindow(app.pid, 1200 * dpr, 900 * dpr);
  expect(wide?.ok).toBe(true);
  await page.waitForTimeout(300);
  const wideLayout = await page.evaluate(() => ({
    width: window.innerWidth,
    scrollWidth: document.documentElement.scrollWidth,
  }));
  expect(wideLayout.scrollWidth).toBeLessThanOrEqual(wideLayout.width + 1);

  // A real account gives the populated views their warning chips, quota
  // cards, and notification rows; audit each one instead of only the
  // first-run body.
  await openSettings(page);
  await createCustomAccount(page, {
    label: "审计账户",
    baseUrl: provider.baseUrl,
    apiKey: KEY,
  });
  await page.waitForTimeout(500);
  expectClean(await runAudit(page), "settings-populated");

  await openOverview(page);
  await page.waitForTimeout(500);
  expectClean(await runAudit(page), "overview-populated");

  await openHistory(page);
  await page.waitForTimeout(500);
  expectClean(await runAudit(page), "history");

  await openNotifications(page);
  await page.waitForTimeout(500);
  expectClean(await runAudit(page), "notifications");
});
