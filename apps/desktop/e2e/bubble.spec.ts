import { expect, test } from "@playwright/test";

/**
 * Bubble form and morph scenarios over the mock bridge (plain-browser dev
 * preview). The preview shares every view-level decision with the native
 * shell — which rings render, how the clip reveal is computed, what a click
 * does — while the OS-level behaviors (taskbar, window drag, edge snap) live
 * in the real-device pass; see SCENARIOS.md for the boundary matrix.
 */
test.describe("bubble form", () => {
  test("renders the bubble dial with two separated rings for windowed quotas", async ({
    page,
  }) => {
    await page.goto("/?window=bubble");
    const bubble = page.locator(".bubble");
    await expect(bubble).toBeVisible();
    // The accessible name carries the whole contract: brand, health, per-
    // window readings, and the action.
    await expect(bubble).toHaveAttribute("aria-label", /TokenOwl.*展开 TokenOwl 面板/s);

    // Two bands on the layered radii, each with its own unlit track, and no
    // shared channel circle underneath.
    const values = page.locator(".quota-rings__value");
    await expect(values).toHaveCount(2);
    await expect(values.nth(0)).toHaveAttribute("r", "21");
    await expect(values.nth(1)).toHaveAttribute("r", "14");
    await expect(page.locator(".quota-rings__track")).toHaveCount(2);
    await expect(page.locator(".quota-rings__channel")).toHaveCount(0);
    // Rings draw the consumed share: the demo 5h window used 128 of 800.
    await expect(page.locator('[data-ring-tone="outer"]')).toHaveAttribute(
      "data-ring-consumed",
      "0.160",
    );
  });

  test("renders the solo-owl empty state without any dial when no provider exists", async ({
    page,
  }) => {
    await page.goto("/?window=bubble&empty=1");
    await expect(page.locator(".bubble")).toBeVisible();
    await expect(page.locator(".bubble__mark--solo")).toBeVisible();
    await expect(page.locator(".quota-rings__track")).toHaveCount(0);
    await expect(page.locator(".quota-rings__value")).toHaveCount(0);
  });

  test("clicking the bubble returns to the dashboard and the morph lands", async ({ page }) => {
    await page.goto("/?window=bubble");
    await page.locator(".bubble").click();
    await expect(page).toHaveURL(/\/$/);
    // The preview plays the same clip reveal as the native shell: the panel
    // is fully present once the morph settles.
    await expect(page.locator(".window")).toBeVisible();
    await expect(page.getByRole("heading", { name: "概览" })).toBeVisible();
  });

  test("the dashboard's Bubble pill collapses back into the bubble", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "Bubble", exact: true }).click();
    await expect(page).toHaveURL(/window=bubble/);
    await expect(page.locator(".bubble")).toBeVisible();
  });

  test("survives ten expand/collapse round trips without wedging", async ({ page }) => {
    await page.goto("/");
    for (let round = 0; round < 10; round += 1) {
      await page.getByRole("button", { name: "Bubble", exact: true }).click();
      await expect(page.locator(".bubble")).toBeVisible();
      await page.locator(".bubble").click();
      await expect(page.locator(".window")).toBeVisible();
    }
    // The dashboard is still interactive after the loop.
    await expect(page.getByRole("heading", { name: "概览" })).toBeVisible();
  });

  test("the bubble window shows no dashboard chrome", async ({ page }) => {
    await page.goto("/?window=bubble");
    // A 56×56 floating window has no dock, no titlebar, no scroll region.
    await expect(page.locator(".dock")).toHaveCount(0);
    await expect(page.locator(".titlebar")).toHaveCount(0);
  });
});
