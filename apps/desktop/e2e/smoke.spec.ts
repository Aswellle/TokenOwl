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
