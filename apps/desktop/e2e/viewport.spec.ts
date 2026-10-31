import { expect, test } from "@playwright/test";

/**
 * Window-size resilience. The real shell can be resized and the bubble can be
 * placed on high-DPI and narrow screens, so the layout contract is checked at
 * several viewports rather than only the 1280×720 default.
 */
const VIEWPORTS = [
  { width: 480, height: 640, name: "narrow" },
  { width: 560, height: 720, name: "compact" },
  { width: 800, height: 600, name: "small" },
  { width: 1440, height: 900, name: "large" },
];

for (const viewport of VIEWPORTS) {
  test(`no horizontal overflow and a single content scroller at ${viewport.name}`, async ({
    page,
  }) => {
    await page.setViewportSize({ width: viewport.width, height: viewport.height });
    await page.goto("/");
    await expect(page.locator(".account")).toHaveCount(4, { timeout: 15_000 });

    const metrics = await page.evaluate(() => ({
      docOverflow:
        document.documentElement.scrollWidth - document.documentElement.clientWidth,
      bodyOverflow: document.body.scrollWidth - document.body.clientWidth,
      scrollers: Array.from(document.querySelectorAll("*"))
        .filter((element) => {
          const style = getComputedStyle(element);
          return (
            element.scrollHeight > element.clientHeight + 4 &&
            element.clientHeight > 40 &&
            (style.overflowY === "auto" || style.overflowY === "scroll")
          );
        })
        .map((element) => String(element.className || element.tagName)),
    }));
    expect(metrics.docOverflow).toBeLessThanOrEqual(1);
    expect(metrics.bodyOverflow).toBeLessThanOrEqual(1);
    expect(metrics.scrollers.length).toBeLessThanOrEqual(1);
    if (metrics.scrollers.length === 1) {
      expect(metrics.scrollers[0]).toContain("content");
    }

    // The shell chrome stays inside the viewport.
    const shell = await page.locator(".window").boundingBox();
    expect(shell).not.toBeNull();
    expect(shell!.x).toBeGreaterThanOrEqual(-1);
    expect(shell!.y).toBeGreaterThanOrEqual(-1);
    expect(shell!.x + shell!.width).toBeLessThanOrEqual(viewport.width + 1);
    expect(shell!.y + shell!.height).toBeLessThanOrEqual(viewport.height + 1);
    await expect(page.getByRole("navigation", { name: "功能导航" })).toBeVisible();
  });
}

test("long labels stay inside a narrow window", async ({ page }) => {
  await page.setViewportSize({ width: 480, height: 640 });
  await page.goto("/?scenario=long");
  await expect(page.locator(".account")).toHaveCount(4, { timeout: 15_000 });
  const overflow = await page.evaluate(() => ({
    doc: document.documentElement.scrollWidth - document.documentElement.clientWidth,
    body: document.body.scrollWidth - document.body.clientWidth,
  }));
  expect(overflow.doc).toBeLessThanOrEqual(1);
  expect(overflow.body).toBeLessThanOrEqual(1);
});

test("the bubble preview keeps its circle inside a small viewport", async ({ page }) => {
  await page.setViewportSize({ width: 320, height: 320 });
  // Reduced motion skips the entrance spring so the box is measured at rest.
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.goto("/?window=bubble");
  const bubble = page.locator(".bubble");
  await expect(bubble).toBeVisible();
  const box = await bubble.boundingBox();
  expect(box).not.toBeNull();
  expect(box!.width).toBeCloseTo(128, 0);
  expect(box!.height).toBeCloseTo(128, 0);
  const overflow = await page.evaluate(() => ({
    doc: document.documentElement.scrollWidth - document.documentElement.clientWidth,
    body: document.body.scrollWidth - document.body.clientWidth,
  }));
  expect(overflow.doc).toBeLessThanOrEqual(1);
  expect(overflow.body).toBeLessThanOrEqual(1);
});

test("the settings form remains usable at the narrowest width", async ({ page }) => {
  await page.setViewportSize({ width: 480, height: 640 });
  await page.goto("/");
  await page.getByRole("button", { name: "设置", exact: true }).click();
  await expect(page.getByRole("heading", { name: "连接 AI 供应商账户" })).toBeVisible();
  await expect(page.getByLabel("账户名称")).toBeVisible();
  await page.getByLabel("账户名称").fill("窄窗口账户");
  await page.getByLabel("接口地址").fill("https://api.example.com/v1");
  await page.getByLabel("API 密钥").fill("sk-test");
  await page.getByRole("button", { name: "连接账户" }).scrollIntoViewIfNeeded();
  await expect(page.getByRole("button", { name: "连接账户" })).toBeInViewport();
  const overflow = await page.evaluate(() => ({
    doc: document.documentElement.scrollWidth - document.documentElement.clientWidth,
    body: document.body.scrollWidth - document.body.clientWidth,
  }));
  expect(overflow.doc).toBeLessThanOrEqual(1);
  expect(overflow.body).toBeLessThanOrEqual(1);
});
