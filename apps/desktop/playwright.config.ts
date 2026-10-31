import { execFileSync } from "node:child_process";
import process from "node:process";
import { defineConfig } from "@playwright/test";

/**
 * E2E smoke against the plain Vite preview of the dashboard (mock bridge,
 * demo data). The Tauri shell needs a GUI and a Rust build — those stay a
 * local/manual verification step per docs/07; what CI can prove is that the
 * webview app renders, navigates, and reads its bridge in a real browser.
 *
 * The port is allocated, not hardcoded: Windows/Hyper-V reserve changing
 * blocks of TCP ports at boot (1877 sat inside 1822–1921 on one machine), and
 * a hardcoded port turns those reservations into EACCES before any test runs.
 */
function freePort(): number {
  const script = [
    "const net = require('node:net');",
    "const server = net.createServer();",
    "server.listen(0, '127.0.0.1', () => {",
    "  process.stdout.write(String(server.address().port));",
    "  server.close();",
    "});",
  ].join("");
  const output = execFileSync(process.execPath, ["-e", script], { encoding: "utf8" });
  const port = Number(output.trim());
  if (!Number.isInteger(port) || port <= 0) {
    throw new Error(`could not allocate an e2e port (got ${JSON.stringify(output)})`);
  }
  return port;
}

// Playwright evaluates this config in the runner and again in every worker;
// a freshly allocated port per evaluation made baseURL disagree with the
// server. Pin the first allocation into the inherited environment.
const PORT = Number(process.env.E2E_PORT ?? freePort());
process.env.E2E_PORT = String(PORT);

export default defineConfig({
  testDir: "./e2e",
  globalSetup: "./e2e/global-setup.ts",
  timeout: 30_000,
  retries: process.env.CI ? 1 : 0,
  workers: 1,
  reporter: process.env.CI ? "github" : "list",
  use: {
    baseURL: `http://127.0.0.1:${PORT}`,
    trace: "retain-on-failure",
  },
  webServer: {
    command: `pnpm dev --port ${PORT} --host 127.0.0.1 --strictPort`,
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
