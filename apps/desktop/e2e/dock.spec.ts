import { expect, test } from "@playwright/test";

/**
 * The bottom dock must be visible at the default startup size (never lost
 * under the OS taskbar / Dock), collapse smoothly to hand its strip back to
 * the content, and stay reachable through one control that is always centred
 * on the dock strip's top edge.
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

  test("the control is centred on the dock strip's top edge", async ({ page }) => {
    await page.goto("/");
    const dock = page.locator(".dock");
    await expect(dock).toBeVisible();
    const tab = page.getByRole("button", { name: "收起控制台" });
    await expect(tab).toBeVisible();

    const geometry = await page.evaluate(() => {
      const tabRect = document.querySelector(".dock-tab")!.getBoundingClientRect();
      const dockRect = document.querySelector(".dock")!.getBoundingClientRect();
      return {
        tabCenter: tabRect.left + tabRect.width / 2,
        tabBottom: tabRect.bottom,
        dockTop: dockRect.top,
        dockCenter: dockRect.left + dockRect.width / 2,
        windowCenter: (document.querySelector(".window") as HTMLElement).getBoundingClientRect().left +
          (document.querySelector(".window") as HTMLElement).getBoundingClientRect().width / 2,
        tabTop: tabRect.top,
      };
    });
    // Horizontally centred on the dock (and therefore on the window).
    expect(Math.abs(geometry.tabCenter - geometry.dockCenter)).toBeLessThan(1.5);
    expect(Math.abs(geometry.tabCenter - geometry.windowCenter)).toBeLessThan(1.5);
    // Flush with the dock's top edge, rising into the content area.
    expect(Math.abs(geometry.tabBottom - geometry.dockTop)).toBeLessThan(1.5);
    expect(geometry.tabTop).toBeLessThan(geometry.dockTop);
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
    // The reclaimed strip went to the content.
    const contentAfter = await page
      .locator(".content")
      .evaluate((el) => el.clientHeight);
    expect(contentAfter).toBeGreaterThan(contentBefore);

    // The control is still there, now resting on the window's bottom edge.
    const tab = page.getByRole("button", { name: "显示控制台" });
    await expect(tab).toBeVisible();
    const geometry = await page.evaluate(() => {
      const tabRect = document.querySelector(".dock-tab")!.getBoundingClientRect();
      const windowRect = document.querySelector(".window")!.getBoundingClientRect();
      return {
        tabBottom: tabRect.bottom,
        tabTop: tabRect.top,
        tabCenter: tabRect.left + tabRect.width / 2,
        windowBottom: windowRect.bottom,
        windowCenter: windowRect.left + windowRect.width / 2,
      };
    });
    // Resting on the window's bottom edge, still centred.
    expect(Math.abs(geometry.tabBottom - geometry.windowBottom)).toBeLessThan(1.5);
    expect(geometry.tabTop).toBeLessThan(geometry.windowBottom);
    expect(Math.abs(geometry.tabCenter - geometry.windowCenter)).toBeLessThan(1.5);
  });

  test("the same control brings the dock back", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "收起控制台" }).click();
    const expand = page.getByRole("button", { name: "显示控制台" });
    await expect(expand).toBeVisible();

    await expand.click();
    await expect(page.locator(".dock")).toBeVisible();
    await expect(page.locator(".dock-wrap--collapsed")).toHaveCount(0);
    // The control stays put and flips to the collapse action.
    const collapse = page.getByRole("button", { name: "收起控制台" });
    await expect(collapse).toBeVisible();
    await expect(collapse).toHaveAttribute("aria-expanded", "true");
    const box = await page.locator(".dock").boundingBox();
    expect(box!.y + box!.height).toBeLessThanOrEqual(
      await page.evaluate(() => window.innerHeight),
    );
  });

  test("the control is keyboard operable", async ({ page }) => {
    await page.goto("/");
    const collapse = page.getByRole("button", { name: "收起控制台" });
    await collapse.focus();
    await page.keyboard.press("Enter");
    await expect(page.locator(".dock-wrap--collapsed")).toHaveCount(1);
    // Focus stays on the same control, which now expands.
    await expect(page.getByRole("button", { name: "显示控制台" })).toBeFocused();
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
