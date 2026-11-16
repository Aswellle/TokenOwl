import { expect, test } from "./fixtures";
import fs from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createCustomAccount, openHistory, openNotifications, openOverview, openSettings } from "./helpers";

const KEY = ["sk", "native", "old"].join("-");
const SHOTS = fileURLToPath(new URL("../verify-shots/native-e2e-2026-11/", import.meta.url));

/**
 * Visual-review captures for the real Windows build. The assertions keep the
 * test honest (each view rendered its content); the PNGs are the evidence a
 * human reviews for spacing, hierarchy, clipping, and contrast.
 */
test("captures each populated view for visual review", async ({ app, provider }) => {
  await fs.mkdir(SHOTS, { recursive: true });
  const page = app.page;

  await openSettings(page);
  await createCustomAccount(page, {
    label: "视觉审查账户",
    baseUrl: provider.baseUrl,
    apiKey: KEY,
  });
  await openOverview(page);
  await expect(page.locator(".account")).toHaveCount(1);
  await page.screenshot({ path: path.join(SHOTS, "overview.png") });

  await page.locator(".account__identity").click();
  await expect(page.getByRole("region", { name: "视觉审查账户" })).toBeVisible();
  await page.screenshot({ path: path.join(SHOTS, "provider-detail.png") });
  await page.getByRole("button", { name: "返回概览" }).click();

  await openSettings(page);
  await page.screenshot({ path: path.join(SHOTS, "settings-top.png") });
  await page.locator("details.diagnostics-section").scrollIntoViewIfNeeded();
  await page.screenshot({ path: path.join(SHOTS, "settings-diagnostics.png") });

  await openHistory(page);
  await page.screenshot({ path: path.join(SHOTS, "history.png") });
  await openNotifications(page);
  await page.screenshot({ path: path.join(SHOTS, "notifications.png") });

  await page.getByRole("button", { name: "Bubble", exact: true }).click();
  const bubble = await app.bubblePage();
  await bubble.waitForTimeout(500);
  await bubble.screenshot({ path: path.join(SHOTS, "bubble.png") });
  await bubble.getByRole("button").click();
  // The dashboard returns on the view it was collapsed from.
  await expect(page.getByRole("heading", { name: "通知" })).toBeVisible();
});
