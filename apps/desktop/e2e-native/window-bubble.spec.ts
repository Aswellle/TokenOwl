import { expect, test } from "./fixtures";
import { waitFor, type NativeWindow } from "./native-app";
import { createCustomAccount, openOverview } from "./helpers";

const KEY = ["sk", "native", "old"].join("-");

function byTitle(windows: NativeWindow[], title: string): NativeWindow | undefined {
  return windows.find((window) => window.title === title);
}

async function visibility(
  app: import("./native-app").NativeApp,
): Promise<{ main?: NativeWindow; bubble?: NativeWindow }> {
  const windows = await app.windows();
  return { main: byTitle(windows, "TokenOwl"), bubble: byTitle(windows, "TokenOwl status") };
}

/**
 * The real two-window handoff: the dashboard collapses into a 56-logical-px
 * tool window, the bubble webview draws the same snapshots, and clicking it
 * brings the dashboard back. Window visibility and styles come from Win32,
 * not from CSS.
 */
test("the dashboard hands off to a 56px tool-window bubble and back", async ({ app, provider }) => {
  const page = app.page;
  const dpr = await page.evaluate(() => window.devicePixelRatio);
  await page.getByRole("button", { name: "设置", exact: true }).click();
  await createCustomAccount(page, {
    label: "Bubble 账户",
    baseUrl: provider.baseUrl,
    apiKey: KEY,
  });
  await openOverview(page);

  await page.getByRole("button", { name: "Bubble", exact: true }).click();
  const visible = await waitFor(async () => {
    const state = await visibility(app);
    return state.main && state.bubble && !state.main.visible && state.bubble.visible
      ? state
      : null;
  }, 15_000);
  const bubbleWindow = visible.bubble!;
  expect(bubbleWindow.exStyle & 0x80).not.toBe(0);
  const expected = 56 * dpr;
  expect(bubbleWindow.width).toBeGreaterThanOrEqual(expected - 2);
  expect(bubbleWindow.width).toBeLessThanOrEqual(expected + 4);
  expect(bubbleWindow.height).toBeGreaterThanOrEqual(expected - 2);
  expect(bubbleWindow.height).toBeLessThanOrEqual(expected + 4);

  const bubble = await app.bubblePage();
  const button = bubble.getByRole("button");
  await expect(button).toHaveAttribute(
    "aria-label",
    /需要关注。额度已用：5h 42%，7d 82%。展开 TokenOwl 面板/,
  );
  await expect(bubble.locator(".bubble__mark")).toBeVisible();
  await expect(bubble.locator('[data-ring-tone="outer"]')).toHaveAttribute(
    "data-ring-consumed",
    "0.420",
  );
  await expect(bubble.locator('[data-ring-tone="inner"]')).toHaveAttribute(
    "data-ring-consumed",
    "0.820",
  );
  await button.click();

  await waitFor(async () => {
    const state = await visibility(app);
    return state.main?.visible && state.bubble && !state.bubble.visible ? state : null;
  }, 15_000);
  await expect(page.getByRole("heading", { name: "概览" })).toBeVisible();

  // The titlebar close control uses the same handoff as the Bubble pill.
  await page.getByRole("button", { name: "关闭面板（回到 Bubble）" }).click();
  await waitFor(async () => {
    const state = await visibility(app);
    return state.main && !state.main.visible && state.bubble?.visible ? state : null;
  }, 15_000);
  await (await app.bubblePage()).getByRole("button").click();
  await waitFor(async () => {
    const state = await visibility(app);
    return state.main?.visible && state.bubble && !state.bubble.visible ? state : null;
  }, 15_000);
  await expect(page.getByRole("heading", { name: "概览" })).toBeVisible();
});
