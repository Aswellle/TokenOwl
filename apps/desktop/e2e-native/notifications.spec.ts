import { expect, test } from "./fixtures";
import {
  createCustomAccount,
  openNotifications,
} from "./helpers";

const KEY = ["sk", "native", "old"].join("-");

/**
 * Alert generation, read state and preferences against a low-quota account.
 * The notification rows come from the real trigger pipeline, not a fixture
 * list, and the preferences are persisted in SQLite.
 */
test("a low-quota refresh creates alerts that can be read and muted", async ({
  app,
  provider,
}) => {
  provider.mode = "low";
  const page = app.page;
  await page.getByRole("button", { name: "设置", exact: true }).click();
  await createCustomAccount(page, {
    label: "低额度账户",
    baseUrl: provider.baseUrl,
    apiKey: KEY,
  });

  await openNotifications(page);
  const items = page.locator(".notifs li");
  await expect(items.first()).toBeVisible({ timeout: 15_000 });
  const total = await items.count();
  expect(total).toBeGreaterThanOrEqual(1);
  await expect(items.first()).toContainText("额度不足");
  await expect(page.locator(".notif__badge")).toContainText(`${total} 条未读`);

  // Mark one read: the badge decrements and that row loses its control.
  await items.first().getByRole("button", { name: /标记通知已读/ }).click();
  await expect(page.locator(".notif__badge")).toContainText(`${total - 1} 条未读`);
  await expect(
    items.first().getByRole("button", { name: /标记通知已读/ }),
  ).toHaveCount(0);

  await page.getByRole("button", { name: "标记所有通知为已读" }).click();
  await expect(page.locator(".notif__badge")).toHaveCount(0, { timeout: 15_000 });
  await expect(page.getByRole("button", { name: /标记通知已读/ })).toHaveCount(0);

  // Preference mutations are real saves; the dependent fieldset locks while
  // alerting is off and unlocks again when it is restored.
  const settings = page.locator("details.notif-settings");
  await settings.locator("summary").click();
  const enabled = settings.getByRole("checkbox", { name: /启用通知/ });
  await expect(enabled).toBeChecked();
  await enabled.uncheck();
  await expect(settings.locator("summary")).toContainText("已停用");
  await expect(settings.locator(".notif-settings__fields")).toHaveAttribute("disabled", "");
  await expect(settings.getByLabel("低额度阈值（% 以下提醒）")).toBeDisabled();
  await expect(settings.getByRole("status")).toContainText("已保存");

  await enabled.check();
  await expect(settings.locator("summary")).toContainText("已启用");
  await expect(settings.getByLabel("低额度阈值（% 以下提醒）")).toBeEnabled();

  // A numeric threshold commits on blur, not on every keystroke.
  const threshold = settings.getByLabel("低额度阈值（% 以下提醒）");
  await expect(threshold).toHaveValue("20");
  await threshold.fill("35");
  await threshold.blur();
  await expect(settings.getByRole("status")).toContainText("已保存");
  await expect(threshold).toHaveValue("35");
});
