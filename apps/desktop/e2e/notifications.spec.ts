import { expect, test, type Page } from "@playwright/test";

async function openNotifications(page: Page, query = ""): Promise<void> {
  await page.goto(`/${query}`);
  await page.getByRole("button", { name: "通知", exact: true }).click();
  await expect(page.locator(".notif-section")).toBeVisible();
}

async function openSettingsDetails(page: Page): Promise<void> {
  const details = page.locator(".notif-settings");
  await expect(details).toHaveCount(1);
  if (!(await details.evaluate((element) => (element as HTMLDetailsElement).open))) {
    await details.locator("summary").click();
  }
  await expect(page.locator(".notif-settings__body")).toBeVisible();
}

test.describe("notification log", () => {
  test("lists the three demo alerts with an unread count and severity labels", async ({
    page,
  }) => {
    await openNotifications(page);
    await expect(page.locator(".notif")).toHaveCount(3);
    await expect(page.locator(".notif__badge")).toHaveText("2 条未读");
    await expect(page.locator(".notif__severity--critical").first()).toHaveText("紧急");
    await expect(page.locator(".notif__severity--warning").first()).toHaveText("警告");
    await expect(page.locator(".notif__severity--info").first()).toHaveText("信息");
    // The kind chip is suppressed when the title already says it.
    await expect(page.locator(".notif").first().locator(".notif__kind")).toHaveCount(0);
  });

  test("marking one alert read removes its button and decrements the badge", async ({
    page,
  }) => {
    await openNotifications(page);
    await page
      .getByRole("button", { name: "标记通知已读：额度紧张" })
      .click();
    await expect(page.locator(".notif__badge")).toHaveText("1 条未读", { timeout: 15_000 });
    await expect(
      page.getByRole("button", { name: "标记通知已读：额度紧张" }),
    ).toHaveCount(0);
    await expect(page.locator(".notif")).toHaveCount(3);
  });

  test("mark all read clears the badge and every per-row control", async ({ page }) => {
    await openNotifications(page);
    await page.getByRole("button", { name: "标记所有通知为已读" }).click();
    await expect(page.locator(".notif__badge")).toHaveCount(0, { timeout: 15_000 });
    await expect(page.locator(".notif__read")).toHaveCount(0);
    await expect(page.locator(".notif")).toHaveCount(3);
  });

  test("the kind filter only offers kinds that are present and filters the log", async ({
    page,
  }) => {
    await openNotifications(page);
    const select = page.getByLabel("类型");
    await expect(select.locator("option")).toHaveCount(4); // all + 3 present kinds
    await select.selectOption("low_quota");
    await expect(page.locator(".notif")).toHaveCount(1);
    await expect(page.locator(".notif__title")).toHaveText("额度紧张");
    await select.selectOption("all");
    await expect(page.locator(".notif")).toHaveCount(3);
  });

  test("an empty log says everything is fine rather than showing a skeleton", async ({
    page,
  }) => {
    await openNotifications(page, "?scenario=notifications-empty");
    await expect(page.locator(".notif-section .section__empty")).toHaveText(
      "一切正常，暂无需要处理的事项。额度告警、预测耗尽与恢复通知会出现在这里。",
    );
    await expect(page.locator(".notif")).toHaveCount(0);
  });

  test("a failed log read is reported as a failure, not as an empty inbox", async ({
    page,
  }) => {
    await openNotifications(page, "?scenario=notifications-fail");
    await expect(page.locator(".notif-section .section__empty")).toContainText(
      "通知暂时无法读取",
      { timeout: 15_000 },
    );
    await expect(page.getByRole("button", { name: "重试" })).toBeVisible();
    await expect(page.getByText("一切正常，暂无需要处理的事项")).toHaveCount(0);
  });

  test("a failed mark-read surfaces an error instead of failing silently", async ({ page }) => {
    await openNotifications(page, "?scenario=notifications-mark-fail");
    await page.getByRole("button", { name: "标记通知已读：额度紧张" }).click();
    await expect(page.locator(".notif-section [role='alert']")).toContainText(
      "标记已读失败",
      { timeout: 15_000 },
    );
    // The alert is still unread; the UI never pretends the write succeeded.
    await expect(page.locator(".notif__badge")).toHaveText("2 条未读");
  });
});

test.describe("notification preferences", () => {
  test("disabling notifications commits immediately and locks the dependent fields", async ({
    page,
  }) => {
    await openNotifications(page);
    await openSettingsDetails(page);
    const enabled = page.getByRole("checkbox", { name: /启用通知/ });
    await expect(enabled).toBeChecked();
    await enabled.uncheck();
    await expect(page.getByRole("status")).toHaveText("已保存", { timeout: 15_000 });
    await expect(page.getByRole("checkbox", { name: /静默模式/ })).toBeDisabled();
    await expect(page.getByLabel(/低额度阈值/)).toBeDisabled();

    await enabled.check();
    await expect(page.getByRole("checkbox", { name: /静默模式/ })).toBeEnabled({
      timeout: 15_000,
    });
  });

  test("number fields commit on blur, not on every keystroke", async ({ page }) => {
    await openNotifications(page);
    await openSettingsDetails(page);
    const threshold = page.getByLabel(/低额度阈值/);
    await threshold.fill("35");
    // Still uncommitted while focused.
    await expect(page.getByRole("status")).toHaveCount(0);
    await threshold.blur();
    await expect(page.getByRole("status")).toHaveText("已保存", { timeout: 15_000 });

    // Leaving and re-entering the view re-reads the persisted preferences
    // from the bridge instead of trusting the local input state.
    await page.getByRole("button", { name: "概览", exact: true }).click();
    await expect(page.getByRole("heading", { name: "概览" })).toBeVisible();
    await expect(page.locator(".notif-section")).toHaveCount(0);
    await page.getByRole("button", { name: "通知", exact: true }).click();
    await expect(page.getByRole("heading", { name: "通知" })).toBeVisible();
    await openSettingsDetails(page);
    await expect(page.getByLabel(/低额度阈值/)).toHaveValue("35");
  });

  test("a rejected save rolls the control back and shows the error", async ({ page }) => {
    await openNotifications(page, "?scenario=prefs-fail");
    await openSettingsDetails(page);
    const enabled = page.getByRole("checkbox", { name: /启用通知/ });
    await expect(enabled).toBeChecked();
    await enabled.uncheck();
    await expect(page.locator(".notif-settings [role='alert']")).toHaveText(
      "保存失败，请检查输入后重试。",
      { timeout: 15_000 },
    );
    await expect(enabled).toBeChecked();
  });

  test("unreadable preferences are explained while the log stays usable", async ({ page }) => {
    await openNotifications(page, "?scenario=prefs-load-fail");
    await expect(page.locator(".notif")).toHaveCount(3);
    await expect(page.locator(".notif-section")).toContainText(
      "通知设置暂时无法读取",
      { timeout: 15_000 },
    );
  });
});
