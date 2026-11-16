import { defineConfig } from "@playwright/test";

/**
 * Native end-to-end suite: drives the real Windows Tauri build over
 * WebView2's CDP endpoint. It is deliberately a separate project from
 * `pnpm e2e` because it needs a GUI session, the OS credential store, and a
 * prior `pnpm tauri build --debug --no-bundle`. Every test spawns its own
 * process and an isolated application-data/keyring namespace.
 */
export default defineConfig({
  testDir: "./e2e-native",
  timeout: 120_000,
  expect: { timeout: 15_000 },
  retries: 0,
  workers: 1,
  fullyParallel: false,
  reporter: [["list"]],
  use: {
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
  },
});
