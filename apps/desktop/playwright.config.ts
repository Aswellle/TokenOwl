import { defineConfig } from "@playwright/test";

/**
 * E2E smoke against the plain Vite preview of the dashboard (mock bridge,
 * demo data). The Tauri shell needs a GUI and a Rust build — those stay a
 * local/manual verification step per docs/07; what CI can prove is that the
 * webview app renders, navigates, and reads its bridge in a real browser.
 */
const PORT = 1800;

export default defineConfig({
  testDir: "./e2e",
  timeout: 30_000,
  retries: process.env.CI ? 1 : 0,
  workers: 1,
  reporter: process.env.CI ? "github" : "list",
  use: {
    baseURL: `http://127.0.0.1:${PORT}`,
    trace: "retain-on-failure",
  },
  webServer: {
    command: "pnpm dev --port 1800 --host 127.0.0.1 --strictPort",
    url: `http://127.0.0.1:${PORT}`,
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
  },
  projects: [
    {
      name: "chromium",
      use: {
        browserName: "chromium",
        // CI installs the pinned Playwright build; locally the managed
        // download is often blocked, so fall back to the system Chrome.
        channel: process.env.PLAYWRIGHT_CHROME_CHANNEL ?? "chrome",
      },
    },
  ],
});
