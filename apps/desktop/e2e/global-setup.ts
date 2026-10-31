import process from "node:process";
import { chromium } from "@playwright/test";

/**
 * Warms the dev server with a real page load before any test runs.
 *
 * Vite optimizes dependencies on demand: the very first browser visit after a
 * cache invalidation can trigger a re-optimization that hard-reloads the page
 * mid-test. The suite's first spec (alphabetically) absorbed those reloads as
 * an intermittent failure that never reproduced in isolation. Loading the two
 * window modes here keeps that reload inside setup, where it is harmless.
 */
export default async function globalSetup(): Promise<void> {
  const browser = await chromium.launch({
    channel: process.env.PLAYWRIGHT_CHROME_CHANNEL ?? "chrome",
  });
  try {
  const port = process.env.E2E_PORT ?? "1420";
  const base = `http://127.0.0.1:${port}`;
  const page = await browser.newPage();
  for (const path of ["/", "/?window=bubble"]) {
    await page.goto(new URL(path, base).toString(), {
      waitUntil: "networkidle",
      timeout: 30_000,
    });
      // A pending re-optimization fires a full-reload shortly after the first
      // render; let it land here instead of inside the first spec.
      await page.waitForTimeout(800);
      await page.waitForLoadState("networkidle");
    }
    await page.close();
  } finally {
    await browser.close();
  }
}
