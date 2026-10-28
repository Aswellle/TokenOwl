import { expect, test } from "@playwright/test";

/**
 * Dashboard smoke over the mock bridge (plain-browser dev preview). The mock
 * only exists under `import.meta.env.DEV`, which is exactly what the dev
 * server serving these tests runs.
 */
test.describe("TokenOwl dashboard smoke", () => {
  test("renders the overview with demo accounts", async ({ page }) => {
    await page.goto("/");
    await expect(page.locator(".window")).toBeVisible();
    await expect(page.getByRole("navigation", { name: "功能导航" })).toBeVisible();
    // The titlebar heading names the live page (概览 here) instead of a
    // static product subtitle.
    await expect(page.getByRole("heading", { name: "概览" })).toBeVisible();
  });

  test("navigates through the dock views", async ({ page }) => {
    await page.goto("/");
    for (const view of ["历史", "通知", "设置"]) {
      await page.getByRole("button", { name: view, exact: true }).click();
      await expect(page.getByRole("button", { name: view, exact: true })).toHaveAttribute(
        "aria-pressed",
        "true",
      );
    }
  });

  test("settings shows the protocol-grouped provider picker", async ({ page }) => {
    await page.goto("/?empty=1");
    await page.getByRole("button", { name: "添加账户" }).click();
    await expect(page.getByText("OpenAI 兼容格式")).toBeVisible();
    await expect(page.getByText("Anthropic 格式")).toBeVisible();
    await expect(page.getByText("Gemini 格式")).toBeVisible();
    await expect(page.getByText("自定义接入", { exact: true })).toBeVisible();
  });

  test("bubble preview renders and returns to the dashboard", async ({ page }) => {
    await page.goto("/?window=bubble");
    const bubble = page.locator(".bubble");
    await expect(bubble).toBeVisible();
    await bubble.click();
    await expect(page).toHaveURL(/\/$/);
    await expect(page.locator(".window")).toBeVisible();
  });
});

test.describe("settings view (phase-C UI)", () => {
  test("capability matrix is collapsed by default and expands on click", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "设置", exact: true }).click();
    const disclosure = page.locator(".matrix__disclosure");
    await expect(disclosure).toBeVisible();
    // Collapsed by default: reference material stays out of the way.
    await expect(disclosure).not.toHaveAttribute("open", "");
    await disclosure.locator("summary").click();
    await expect(disclosure).toHaveAttribute("open", "");
    await expect(page.locator(".matrix__row").first()).toBeVisible();
  });

  test("the anthropic brand collapses its format variants behind the ellipsis", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "设置", exact: true }).click();
    const anthropicGroup = page.locator(".picker__heading", { hasText: "Anthropic 格式" });
    await expect(anthropicGroup).toBeVisible();
    // The brand capsule carries the ellipsis expander; the OpenAI-compatible
    // relay lives in its variant menu instead of a second capsule.
    await expect(page.getByRole("button", { name: "Anthropic", exact: true })).toBeVisible();
    const more = page.getByRole("button", { name: "Anthropic 的其他平台版本" });
    await more.click();
    await expect(page.getByRole("option", { name: /OpenAI 兼容/ })).toBeVisible();
  });

  test("the update check result appears beside the check button, in view", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "设置", exact: true }).click();
    const check = page.getByRole("button", { name: "检查更新" });
    await check.scrollIntoViewIfNeeded();
    await check.click();
    const banner = page.getByRole("status").filter({ hasText: "已是最新版本" });
    await expect(banner).toBeVisible();
    // The settings form is taller than the window: the result must be
    // readable where the user pressed the button, without scrolling back up.
    await expect(banner).toBeInViewport();
  });
});

test.describe("viewport scrolling", () => {
  test("only the content region scrolls — the shell is pinned", async ({ page }) => {
    await page.goto("/");
    const scrollers = await page.evaluate(() => {
      const found: Array<{ cls: string; sh: number; ch: number }> = [];
      for (const el of Array.from(document.querySelectorAll("*"))) {
        if (el.scrollHeight > el.clientHeight + 4 && el.clientHeight > 40) {
          const cs = getComputedStyle(el);
          if (cs.overflowY === "auto" || cs.overflowY === "scroll") {
            found.push({ cls: String(el.className || el.tagName), sh: el.scrollHeight, ch: el.clientHeight });
          }
        }
      }
      return found;
    });
    // At most one vertical scroller, and when content overflows it is the
    // dashboard's own region — never the window frame or the document.
    expect(scrollers.length).toBeLessThanOrEqual(1);
    if (scrollers.length === 1) {
      expect(scrollers[0].cls).toContain("content");
    }
    // The window frame itself never overflows its viewport.
    const overflow = await page.evaluate(() => ({
      doc: document.documentElement.scrollHeight - document.documentElement.clientHeight,
      body: document.body.scrollHeight - document.body.clientHeight,
    }));
    expect(overflow.doc).toBeLessThanOrEqual(1);
    expect(overflow.body).toBeLessThanOrEqual(1);
  });
});
