import { expect, test, type Page } from "@playwright/test";

/**
 * Dashboard surfaces over the mock bridge: overview states, filters, row
 * actions, provider detail, refresh/delete failure paths, loading and offline.
 * The mock latency is real (180–440 ms), so every assertion waits for data
 * instead of reading a skeleton frame.
 */
async function openDashboard(page: Page, query = ""): Promise<void> {
  await page.goto(`/${query}`);
  await expect(page.locator(".window")).toBeVisible();
}

test.describe("overview summary", () => {
  test("renders pressure, account counts and the next reset from real snapshots", async ({
    page,
  }) => {
    await openDashboard(page);
    await expect(page.locator(".account")).toHaveCount(4);

    // The tightest remaining ratio is the relay's 680k/2.5M = 27.2%.
    const pressure = page.locator(".stat", { hasText: "额度压力" });
    await expect(pressure.locator(".stat__value")).toHaveText("27%", { timeout: 15_000 });
    await expect(pressure.locator(".stat__hint")).toHaveText("已知窗口");

    const accounts = page.locator(".stat", { hasText: "账户" }).first();
    await expect(accounts.locator(".stat__value")).toHaveText("4");

    const healthy = page.locator(".stat", { hasText: "正常" });
    await expect(healthy.locator(".stat__value")).toHaveText("2");
    await expect(healthy.locator(".stat__hint")).toHaveText("2 个需要处理");

    // The relay's reset in ~41 minutes is the earliest known window.
    const reset = page.locator(".stat", { hasText: "下次重置" });
    await expect(reset.locator(".stat__value")).toHaveText(/^\d+ 分钟$/);
  });

  test("the attention filter keeps only accounts that need work", async ({ page }) => {
    await openDashboard(page);
    const segmented = page.getByRole("group", { name: "按状态筛选账户" });
    await segmented.getByRole("button", { name: /全部/ }).click();
    await expect(page.locator(".account")).toHaveCount(4);

    await segmented.getByRole("button", { name: /需处理/ }).click();
    await expect(page.locator(".account")).toHaveCount(2);
    await expect(page.locator(".account__label")).toHaveText(["公司中转站", "备用网关"]);

    await segmented.getByRole("button", { name: /全部/ }).click();
    await expect(page.locator(".account")).toHaveCount(4);
  });

  test("an account in syncing state never counts as needing attention", async ({ page }) => {
    await openDashboard(page, "?scenario=syncing");
    await expect(page.locator(".account")).toHaveCount(4);
    await expect(page.locator(".chip").first()).toHaveText(/正在同步/);

    await page.getByRole("group", { name: "按状态筛选账户" }).getByRole("button", { name: /需处理/ }).click();
    await expect(page.locator(".section__empty")).toHaveText(
      "没有需要关注的账户——所有供应商都在正常上报。",
    );
  });

  test("an error account keeps its last good snapshot and shows a danger notice", async ({
    page,
  }) => {
    await openDashboard(page);
    const backup = page.locator(".account", { hasText: "备用网关" });
    await expect(backup.locator(".notice--danger")).toContainText(
      "刷新失败，下方显示的是最近一次成功的数据。",
    );
    // The cached snapshot is still visible — an error never blanks the row.
    await expect(backup.locator(".quota")).toHaveCount(1);
    await expect(backup.locator(".chip")).toHaveText(/同步失败/);
    // Overall health is the worst account.
    await expect(page.locator(".titlebar .chip")).toHaveText(/同步失败/);
  });

  test("a stale account shows the warning notice but keeps its values", async ({ page }) => {
    await openDashboard(page, "?scenario=stale");
    const first = page.locator(".account").first();
    await expect(first.locator(".notice--warning")).toContainText("该快照已偏旧");
    await expect(first.locator(".quota")).toHaveCount(2);
    await expect(first.locator(".account__foot")).toContainText("数据偏旧");
  });

  test("a failed first refresh explains the absence of data instead of pointing at old values", async ({
    page,
  }) => {
    await openDashboard(page, "?scenario=error-no-snapshots");
    const first = page.locator(".account").first();
    await expect(first.locator(".notice--danger")).toContainText("首次刷新失败");
    await expect(first.locator(".notice--danger")).not.toContainText("最近一次成功的数据");
    await expect(first.locator(".notice--inline")).toHaveCount(0);
    await expect(first.locator(".quota")).toHaveCount(0);

    await page.locator(".account__identity").first().click();
    await expect(page.locator(".pview .notice--inline")).toContainText("首次刷新失败");
  });

  test("a balance-only account shows the currency balance without inventing a ratio", async ({
    page,
  }) => {
    await openDashboard(page, "?scenario=balance-only");
    const row = page.locator(".account").first();
    await expect(row).toContainText("余额");
    await expect(row).toContainText("CNY -0.23");
    await expect(row.locator(".chip")).toHaveText(/额度紧张/);
    // No limit means no ratio bar, and the pressure stat must say why rather
    // than claiming it is still waiting for the first data.
    await expect(row.getByRole("img")).toHaveCount(0);
    await expect(
      page.locator(".stat", { hasText: "额度压力" }).locator(".stat__hint"),
    ).toHaveText("未上报窗口额度");
  });

  test("quota bars expose the remaining percentage to assistive tech", async ({ page }) => {
    await openDashboard(page);
    const firstBar = page.locator(".account").first().getByRole("img").first();
    await expect(firstBar).toHaveAttribute("aria-label", "5 小时用量窗口：剩余 84%");
  });

  test("partial and failed metrics are labelled instead of silently omitted", async ({
    page,
  }) => {
    await openDashboard(page, "?scenario=partial");
    const row = page.locator(".account").first();
    await expect(row.getByText("数据不完整")).toHaveCount(2);
    await expect(row.getByText("该指标读取失败")).toHaveCount(1);
    // The trusted partial window still draws a bar; the failed one is still
    // shown with its last value rather than zeroed.
    await expect(row.getByRole("img")).toHaveCount(2);
  });
});

test.describe("provider detail", () => {
  test("opens from the row identity with live usage and quota cards", async ({ page }) => {
    await openDashboard(page);
    await page.locator(".account__identity").first().click();
    await expect(page.getByRole("heading", { name: "OpenAI 官方" })).toBeVisible();
    await expect(page.locator(".pview")).toHaveAttribute("aria-label", "OpenAI 官方");
    // The demo primary is prefix `openai`; the manifest's honest level is L2
    // (models + health; the admin Usage API needs query params a plain path
    // cannot express), corrected from the earlier fabricated L4 endpoints.
    await expect(page.locator(".pview .cap-chip")).toHaveText("L2 · 健康/模型");

    // Usage settles from "—" to a real value.
    const tokens = page.locator(".pview__number").first().locator("dd");
    await expect(tokens).not.toHaveText("—", { timeout: 15_000 });
    await expect(page.locator(".pview__trend svg")).toBeVisible();

    // Both real quota windows are present with their own countdowns.
    await expect(page.locator(".pview .quota")).toHaveCount(2);
    await expect(page.locator(".pview .quota__name").first()).toContainText("5 小时用量窗口");
    await expect(page.locator(".pview .quota__reset").first()).toContainText("后重置");
  });

  test("quota kinds render with type-aware cards and grouped sections", async ({ page }) => {
    await openDashboard(page, "?scenario=kinds");
    await page.locator(".account__identity").first().click();
    await expect(page.locator(".pview")).toBeVisible();

    // Both section headers appear: windowed dials and windowless figures.
    await expect(page.getByRole("heading", { name: "窗口额度" })).toBeVisible();
    await expect(page.getByRole("heading", { name: "余额与用量" })).toBeVisible();

    // Window chips carry the type AND the span tag.
    await expect(
      page.locator(".quota", { hasText: "5 小时用量窗口" }).locator(".quota__window"),
    ).toHaveText(/滚动窗口 · 5h/);
    await expect(
      page.locator(".quota", { hasText: "每周额度" }).locator(".quota__window"),
    ).toHaveText(/固定窗口 · 7d/);

    // Tokens use the 万/亿 scale; the raw six-digit count would force counting.
    await expect(
      page.locator(".quota", { hasText: "5 小时用量窗口" }).locator(".quota__values"),
    ).toContainText("50 万");
    await expect(
      page.locator(".quota", { hasText: "输入 Token 用量" }).locator(".quota__values"),
    ).toContainText("1,823.46 万");

    // Currency balances carry the currency and never invent a ratio bar.
    const cash = page.locator(".quota", { hasText: "现金余额" });
    await expect(cash).toContainText("CNY 3");
    await expect(cash.locator(".bar")).toHaveCount(0);
    // Credits read as points with their own subscription period chip.
    await expect(
      page.locator(".quota", { hasText: "积分" }).locator(".quota__window"),
    ).toHaveText(/订阅周期/);
  });

  test("switching the usage window updates the selected tab and the numbers", async ({
    page,
  }) => {
    await openDashboard(page);
    await page.locator(".account__identity").first().click();
    const tabs = page.getByRole("group", { name: "切换统计窗口" });
    await expect(tabs.getByRole("button", { name: "24 小时" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    const tokens = page.locator(".pview__number").first().locator("dd");
    await expect(tokens).not.toHaveText("—", { timeout: 15_000 });
    const before = await tokens.textContent();

    await tabs.getByRole("button", { name: "30 天" }).click();
    await expect(tabs.getByRole("button", { name: "30 天" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    await expect(tokens).not.toHaveText(before ?? "", { timeout: 15_000 });
  });

  test("Escape and the back control both return to the overview", async ({ page }) => {
    await openDashboard(page);
    await page.locator(".account__identity").first().click();
    await expect(page.getByRole("heading", { name: "OpenAI 官方" })).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.getByRole("heading", { name: "概览" })).toBeVisible();

    await page.locator(".account__identity").first().click();
    await page.getByRole("button", { name: "返回概览" }).click();
    await expect(page.getByRole("heading", { name: "概览" })).toBeVisible();
  });

  test("an account without snapshots explains the empty state instead of drawing zeroes", async ({
    page,
  }) => {
    await openDashboard(page, "?scenario=no-snapshots");
    await page.locator(".account__identity").first().click();
    await expect(page.locator(".pview .notice--inline")).toContainText(
      "点击刷新以获取该账户的第一个快照。",
    );
    await expect(page.locator(".pview .quota")).toHaveCount(0);
  });

  test("a failed usage query leaves the cards at — rather than claiming no records", async ({
    page,
  }) => {
    await openDashboard(page, "?scenario=usage-fail");
    await page.locator(".account__identity").first().click();
    await expect(page.locator(".pview__number").first().locator("dd")).toHaveText("—");
    // The quota cards come from the account snapshot and still render.
    await expect(page.locator(".pview .quota")).toHaveCount(2);
  });
});

test.describe("row actions", () => {
  test("refresh all spins, then settles without losing rows", async ({ page }) => {
    await openDashboard(page);
    const refresh = page.getByRole("button", { name: "全部刷新" });
    await refresh.click();
    await expect(refresh).toBeDisabled();
    await expect(page.locator(".account")).toHaveCount(4, { timeout: 20_000 });
    await expect(refresh).toBeEnabled();
  });

  test("a failed refresh keeps every row, marks it error and explains why", async ({ page }) => {
    await openDashboard(page, "?scenario=refresh-fail");
    await page.getByRole("button", { name: "全部刷新" }).click();
    await expect(page.locator(".content > .notice--warning")).toContainText(
      "模拟刷新失败：上游 503",
      { timeout: 20_000 },
    );
    await expect(page.locator(".account")).toHaveCount(4);
    await expect(page.locator(".account .chip").first()).toHaveText(/同步失败/);
  });

  test("removal asks first, cancels safely, and deletes only on confirm", async ({ page }) => {
    await openDashboard(page);
    await page.getByRole("button", { name: "移除账户 OpenAI 官方" }).click();
    const dialog = page.getByRole("alertdialog");
    await expect(dialog).toContainText("移除「OpenAI 官方」？");
    await dialog.getByRole("button", { name: "取消" }).click();
    await expect(dialog).toBeHidden();
    await expect(page.locator(".account")).toHaveCount(4);

    await page.getByRole("button", { name: "移除账户 OpenAI 官方" }).click();
    await expect(page.getByRole("alertdialog")).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.getByRole("alertdialog")).toBeHidden();
    await expect(page.locator(".account")).toHaveCount(4);

    await page.getByRole("button", { name: "移除账户 OpenAI 官方" }).click();
    await page.getByRole("alertdialog").getByRole("button", { name: "确认移除" }).click();
    await expect(page.locator(".account")).toHaveCount(3, { timeout: 15_000 });
    await expect(page.locator(".account__label", { hasText: "OpenAI 官方" })).toHaveCount(0);
  });

  test("a failed removal keeps the account and shows a danger notice", async ({ page }) => {
    await openDashboard(page, "?scenario=delete-fail");
    await page.getByRole("button", { name: "移除账户 OpenAI 官方" }).click();
    await page.getByRole("alertdialog").getByRole("button", { name: "确认移除" }).click();
    await expect(page.locator(".content > .notice--danger")).toContainText(
      "移除失败，请稍后重试。",
      { timeout: 15_000 },
    );
    await expect(page.locator(".account")).toHaveCount(4);
  });
});

test.describe("empty, offline and loading states", () => {
  test("first run explains the empty state and routes to the setup form", async ({ page }) => {
    await openDashboard(page, "?empty=1");
    await expect(page.locator(".empty__title")).toHaveText("连接你的第一个供应商");
    await expect(page.locator(".account")).toHaveCount(0);
    await expect(page.locator(".rail")).toHaveCount(0);
    // Zero accounts is "not connected", never a green healthy badge.
    await expect(page.locator(".titlebar .chip")).toHaveText(/尚未连接/);
    await page.getByRole("button", { name: "添加账户" }).click();
    await expect(page.getByRole("heading", { name: "连接 AI 供应商账户" })).toBeVisible();
  });

  test("offline shows the recoverable local-service state, not a blank shell", async ({
    page,
  }) => {
    await openDashboard(page, "?scenario=offline");
    await expect(page.locator(".empty__title")).toHaveText("本地服务不可用");
    await expect(page.locator(".empty__body")).toContainText("不需要重新添加");
    await expect(page.getByRole("button", { name: "重试" })).toBeVisible();
    await expect(page.locator(".titlebar .chip")).toHaveText(/本地服务不可用/);
    // The global warning notice is redundant with the empty state.
    await expect(page.locator(".content > .notice")).toHaveCount(0);
  });

  test("the first load shows skeletons, then replaces them with rows", async ({ page }) => {
    await page.goto("/?scenario=slow");
    await expect(page.getByLabel("正在加载账户")).toBeVisible();
    await expect(page.locator(".skeleton").first()).toBeVisible();
    await expect(page.locator(".account")).toHaveCount(4, { timeout: 20_000 });
    await expect(page.getByLabel("正在加载账户")).toHaveCount(0);
  });

  test("more than five providers collapse into a counted rail hint", async ({ page }) => {
    await openDashboard(page, "?scenario=many");
    await expect(page.locator(".account")).toHaveCount(8);
    const hint = page.locator(".rail__hint");
    await expect(hint).toHaveText("+3");
    await expect(hint).toHaveAttribute("aria-label", "更多供应商在「全部供应商」面板中");
    await expect(page.locator(".rail__item:not(.rail__item--all)")).toHaveCount(5);
  });
});

test.describe("layout resilience", () => {
  test("long account names never widen the document", async ({ page }) => {
    await openDashboard(page, "?scenario=long");
    const overflow = await page.evaluate(() => ({
      doc: document.documentElement.scrollWidth - document.documentElement.clientWidth,
      body: document.body.scrollWidth - document.body.clientWidth,
    }));
    expect(overflow.doc).toBeLessThanOrEqual(1);
    expect(overflow.body).toBeLessThanOrEqual(1);
    // The full label stays available to assistive tech and on the tooltip.
    await expect(page.locator(".account__label").first()).toContainText("非常非常长的账户名称");
  });
});
