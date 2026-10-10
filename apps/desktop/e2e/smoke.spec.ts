import { test, expect, type ChildProcess } from "@playwright/test";
import { spawn } from "node:child_process";
import { setTimeout as delay } from "node:timers/promises";

let appProcess: ChildProcess | null = null;

async function startApp(): Promise<void> {
  appProcess = spawn("pnpm", ["tauri", "dev"], {
    cwd: __dirname,
    env: { ...process.env, TAURI_DEV_HOST: "127.0.0.1" },
    stdio: "pipe",
  });
  await delay(5000);
}

async function stopApp(): Promise<void> {
  if (appProcess) {
    appProcess.kill();
    appProcess = null;
  }
}

test.describe("TokenOwl E2E smoke test", () => {
  test.beforeAll(async () => {
    await startApp();
  });

  test.afterAll(async () => {
    await stopApp();
  });

  test("app starts and renders main window", async ({ page }) => {
    await page.goto("http://127.0.0.1:1800");
    await expect(page.locator(".window")).toBeVisible();
  });
});
