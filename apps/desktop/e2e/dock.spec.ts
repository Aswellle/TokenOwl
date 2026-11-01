import { expect, test } from "@playwright/test";

/**
 * The bottom dock must be visible at the default startup size (never lost
 * under the OS taskbar / Dock), collapse smoothly to hand its strip back to
 * the content, and come back through an always-reachable floating handle.
 */
test.describe("bottom dock", () => {
  test("the dock is visible and fully inside the window at startup", async ({
    page,
  }) => {
    await page.goto("/");
    const dock = page.locator(".dock");
    await expect(dock).toBeVisible();
    const box = await dock.boundingBox();
    expect(box).not.toBeNull();
    expect(box!.y).toBeGreaterThanOrEqual(0);
    expect(box!.y + box!.height).toBeLessThanOrEqual(await page.evaluate(() => window.innerHeight));
  });

  test("collapsing hides the dock and hands its strip to the content", async ({
    page,
  }) => {
    await page.goto("/");
    const dock = page.locator(".dock");
    await expect(dock).toBeVisible();
    const contentBefore = await page
      .locator(".content")
      .evaluate((el) => el.clientHeight);

    await page.getByRole("button", { name: "收起控制台" }).click();
    // The collapsed wrap has zero height by design, so assert its state
    // rather than visibility.
    await expect(page.locator(".dock-wrap--collapsed")).toHaveCount(1);
    await page.waitForTimeout(450);
    // The dock row itself is clipped away and dropped from the tab order.
    await expect(dock).not.toBeVisible();
    await expect(page.locator("#dock")).toHaveAttribute("inert");
    // The reclaimed strip went to the content, minus the handle's share.
    const contentAfter = await page
      .locator(".content")
      .evaluate((el) => el.clientHeight);
    expect(contentAfter).toBeGreaterThan(contentBefore);

    // The floating handle takes over at the bottom edge.
    const handle = page.getByRole("button", { name: "显示控制台" });
    await expect(handle).toBeVisible();
    const handleBox = await handle.boundingBox();
    expect(handleBox).not.toBeNull();
    expect(handleBox!.y + handleBox!.height).toBeLessThanOrEqual(
      await page.evaluate(() => window.innerHeight),
    );
  });

  test("the floating handle brings the dock back", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "收起控制台" }).click();
    const handle = page.getByRole("button", { name: "显示控制台" });
    await expect(handle).toBeVisible();

    await handle.click();
    await expect(page.locator(".dock")).toBeVisible();
    await expect(handle).toHaveCount(0);
    await expect(page.locator(".dock-wrap--collapsed")).toHaveCount(0);
    // The dock row is back within the window bounds.
    const box = await page.locator(".dock").boundingBox();
    expect(box!.y + box!.height).toBeLessThanOrEqual(
      await page.evaluate(() => window.innerHeight),
    );
  });

  test("the handle is keyboard operable", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "收起控制台" }).click();
    const handle = page.getByRole("button", { name: "显示控制台" });
    await expect(handle).toBeVisible();

    await handle.focus();
    await page.keyboard.press("Enter");
    await expect(page.locator(".dock")).toBeVisible();
  });

  test("collapse state follows the user across view switches", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "收起控制台" }).click();
    await expect(page.locator(".dock-wrap--collapsed")).toHaveCount(1);

    // Bring the dock back, switch views, and collapse again: the preference
    // is per-app, not per-view, and every path keeps working.
    await page.getByRole("button", { name: "显示控制台" }).click();
    await expect(page.locator(".dock")).toBeVisible();
    await page.getByRole("button", { name: "历史" }).click();
    await expect(page.getByRole("heading", { name: "历史" })).toBeVisible();
    await page.getByRole("button", { name: "收起控制台" }).click();
    await expect(page.locator(".dock-wrap--collapsed")).toHaveCount(1);
    await expect(page.getByRole("button", { name: "显示控制台" })).toBeVisible();
  });
});
