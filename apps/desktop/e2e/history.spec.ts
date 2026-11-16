import { expect, test, type Page } from "@playwright/test";

async function openHistory(page: Page, query = ""): Promise<void> {
  await page.goto(`/${query}`);
  await page.getByRole("button", { name: "历史", exact: true }).click();
  await expect(page.locator(".history-section")).toBeVisible();
}

test.describe("history", () => {
  test("summarises the last 30 days, draws the trend and the full heatmap", async ({
    page,
  }) => {
    await openHistory(page);
    const stats = page.locator(".history__stat");
    await expect(stats).toHaveCount(3);
    await expect(stats.nth(0)).toContainText("近 30 天 Tokens");
    await expect(stats.nth(0).locator("strong")).not.toHaveText("0");
    await expect(stats.nth(1).locator("strong")).toHaveText(/^¥/);
    await expect(stats.nth(2).locator("em")).toHaveText("/30");

    await expect(page.locator(".history__trend")).toBeVisible();
    await expect(page.locator(".history__trend-head")).toContainText("峰值");
    await expect(page.locator(".history__trend-chart svg")).toBeVisible();

    // One button per elapsed day, plus a legend of five swatches.
    const cells = page.locator(".heatmap__grid .heat-cell");
    const count = await cells.count();
    expect(count).toBeGreaterThanOrEqual(182);
    expect(count).toBeLessThanOrEqual(190);
    // The useful edge is today, not six months ago: when the grid overflows
    // the heatmap opens scrolled to the right.
    await expect
      .poll(() =>
        page
          .locator(".heatmap__scroll")
          .evaluate((element) => element.scrollLeft + element.clientWidth >= element.scrollWidth - 1),
      )
      .toBe(true);
    await expect(page.locator(".heatmap__legend .heat-swatch")).toHaveCount(5);
    await expect(page.locator(".heatmap__hint")).toHaveText("用方向键查看每一天的用量");
  });

  test("every day cell names its date and either tokens or the no-record state", async ({
    page,
  }) => {
    await openHistory(page);
    const titled = page.locator(".heatmap__grid .heat-cell[title*='月']");
    await expect(titled.first()).toBeVisible();
    const titles = await titled.evaluateAll((nodes) => nodes.map((node) => node.getAttribute("title") ?? ""));
    expect(titles.some((title) => title.includes("Tokens"))).toBe(true);
    expect(titles.some((title) => title.includes("无记录"))).toBe(true);
    // The same contract is exposed to assistive tech, not just the tooltip.
    await expect(page.locator(".heatmap__grid .sr-only").first()).toContainText(/月\d+日/);
  });

  test("arrow keys walk the heatmap and move real focus", async ({ page }) => {
    await openHistory(page);
    const group = page.getByRole("group", { name: "每日用量热力图" });
    await group.focus();
    await page.keyboard.press("ArrowLeft");
    const first = await page.evaluate(() => document.activeElement?.getAttribute("title") ?? "");
    expect(first).toContain("月");

    await page.keyboard.press("ArrowRight");
    const second = await page.evaluate(() => document.activeElement?.getAttribute("title") ?? "");
    expect(second).toContain("月");
    expect(second).not.toBe(first);
  });

  test("the trend tooltip carries the day total, per-provider split and quotas", async ({
    page,
  }) => {
    await openHistory(page);
    const chart = page.locator(".history__trend-chart");
    await chart.scrollIntoViewIfNeeded();
    const box = await chart.boundingBox();
    expect(box).not.toBeNull();
    await page.mouse.move(box!.x + box!.width * 0.75, box!.y + box!.height * 0.5);
    const tip = page.locator(".trend-tip");
    await expect(tip).toBeVisible({ timeout: 10_000 });
    await expect(tip).toContainText("Tokens");
    await expect(tip).toContainText("当前额度");
    await expect(tip.locator(".trend-tip__row")).not.toHaveCount(0);
  });

  test("an empty history explains how recording starts instead of drawing zeroes", async ({
    page,
  }) => {
    await openHistory(page, "?scenario=history-empty");
    await expect(page.locator(".history-section .section__empty")).toHaveText(
      "历史从每次自动刷新开始逐日累积。添加一个账户并等待下一次刷新，这里就会出现第一天的用量。",
    );
    await expect(page.locator(".heatmap__grid")).toHaveCount(0);
    await expect(page.locator(".history__trend")).toHaveCount(0);
  });

  test("a failed history query reports the failure and can be retried", async ({ page }) => {
    await openHistory(page, "?scenario=history-fail");
    await expect(page.locator(".history-section .section__empty")).toContainText(
      "历史数据暂时无法读取",
      { timeout: 15_000 },
    );
    await expect(page.getByRole("button", { name: "重试" })).toBeVisible();
    // No infinite skeleton: the failure is a terminal, explained state.
    await expect(page.locator(".history-section .skeleton")).toHaveCount(0);
  });

  test("exports the visible history as CSV and JSON from the history view", async ({ page }) => {
    await openHistory(page);
    await page.getByRole("button", { name: "导出 CSV" }).click();
    // The mock bridge answers with a path; the section reports it as a
    // status line so the user knows where the file went.
    await expect(page.getByRole("status").filter({ hasText: "已导出到" })).toBeVisible();
    await page.getByRole("button", { name: "导出 JSON" }).click();
    await expect(page.getByRole("status").filter({ hasText: "已导出到" })).toBeVisible();
  });

  test("the local usage section is separate, opt-in, and explains its privacy contract", async ({
    page,
  }) => {
    await openHistory(page);
    const section = page.locator(".local-usage-section");
    await expect(section).toContainText("本地 Agent 使用量");
    // Domain separation is stated, never implied.
    await expect(section).toContainText("与远端 API 额度是完全独立的数据域");

    // A source card spells out its directory, what it collects, and what it
    // never collects before the toggle can be used.
    const claude = section.locator(".local-usage__source").first();
    await expect(claude).toContainText("~/.claude/projects");
    await expect(claude).toContainText("绝不采集");
    await expect(claude).toContainText("提示词正文");

    // Off by default: the toggle is unchecked and scanning is unavailable.
    const toggle = section.getByRole("checkbox", { name: /启用 Claude Code/ });
    await expect(toggle).not.toBeChecked();
    await expect(section.getByRole("button", { name: "立即扫描" })).toBeDisabled();

    // Enabling the source makes the scan action available and persists the
    // choice through the bridge (no hidden local state).
    await toggle.check();
    await expect(section.getByRole("button", { name: "立即扫描" })).toBeEnabled();
    await section.getByRole("button", { name: "立即扫描" }).click();

    // With no stored events the section says so instead of showing zeroes.
    await expect(section).toContainText("暂无本地使用量数据");
  });
});
