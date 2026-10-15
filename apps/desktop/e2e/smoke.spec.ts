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
    await expect(page.getByRole("heading", { name: /额度总览/ })).toBeVisible();
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
    await expect(page.getByText("OpenAI 兼容协议")).toBeVisible();
    await expect(page.getByText("其他原生协议（适配逐步开放）")).toBeVisible();
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

  test("both protocol groups list the OpenAI-compatible entries", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "设置", exact: true }).click();
    const compatGroup = page.locator(".picker__heading").first();
    await expect(compatGroup).toHaveText("OpenAI 兼容协议");
    // anthropic-openai and gemini-openai live in the compatible group.
    const row = compatGroup.locator("xpath=following-sibling::div[1]");
    await expect(row.getByRole("button", { name: /Anthropic \(OpenAI 兼容\)/ })).toBeVisible();
    await expect(row.getByRole("button", { name: /Google Gemini \(OpenAI 兼容\)/ })).toBeVisible();
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
