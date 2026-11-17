import { execFile, spawn, type ChildProcess } from "node:child_process";
import { existsSync } from "node:fs";
import fs from "node:fs/promises";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import { chromium, type Browser, type Page } from "@playwright/test";

const execFileAsync = promisify(execFile);
const EXE = fileURLToPath(new URL("../src-tauri/target/debug/tokenowl.exe", import.meta.url));
const WINDOW_SCRIPT = fileURLToPath(new URL("./windows-window.ps1", import.meta.url));
const RESIZE_SCRIPT = fileURLToPath(new URL("./windows-resize.ps1", import.meta.url));

export interface NativeDirs {
  root: string;
  appData: string;
  home: string;
  webview: string;
  credentialService: string;
}

export interface NativeWindow {
  handle: number;
  title: string;
  visible: boolean;
  x: number;
  y: number;
  width: number;
  height: number;
  style: number;
  exStyle: number;
}

export interface NativeApp {
  process: ChildProcess;
  pid: number;
  browser: Browser;
  page: Page;
  dirs: NativeDirs;
  port: number;
  logs(): string;
  bubblePage(timeoutMs?: number): Promise<Page>;
  windows(): Promise<NativeWindow[]>;
  close(options?: { keepDirs?: boolean }): Promise<void>;
}

async function freePort(): Promise<number> {
  const server = net.createServer();
  await new Promise<void>((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => resolve());
  });
  const port = (server.address() as net.AddressInfo).port;
  await new Promise<void>((resolve) => server.close(() => resolve()));
  return port;
}

async function sleep(ms: number): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, ms));
}

async function createDirs(): Promise<NativeDirs> {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "tokenowl-native-"));
  const dirs = {
    root,
    appData: path.join(root, "app-data"),
    home: path.join(root, "home"),
    webview: path.join(root, "webview2"),
    credentialService: `com.tokenowl.test.native.${process.pid}.${Date.now()}`,
  };
  await Promise.all([
    fs.mkdir(dirs.appData, { recursive: true }),
    fs.mkdir(dirs.home, { recursive: true }),
    fs.mkdir(dirs.webview, { recursive: true }),
  ]);
  return dirs;
}

/** Creates an isolated application-data/home/WebView tree before launch. */
export async function createNativeDirs(): Promise<NativeDirs> {
  return createDirs();
}

export async function waitFor<T>(
  read: () => Promise<T | null>,
  timeoutMs: number,
  intervalMs = 150,
): Promise<T> {
  const deadline = Date.now() + timeoutMs;
  let lastError: unknown = null;
  while (Date.now() < deadline) {
    try {
      const value = await read();
      if (value !== null) return value;
    } catch (error) {
      lastError = error;
    }
    await sleep(intervalMs);
  }
  throw new Error(`condition not met within ${timeoutMs}ms${lastError ? `: ${String(lastError)}` : ""}`);
}

async function waitForMainPage(browser: Browser, timeoutMs: number): Promise<Page> {
  return waitFor(async () => {
    for (const page of browser.contexts().flatMap((context) => context.pages())) {
      try {
        const mode = await page.evaluate(() => document.body?.dataset.window ?? null);
        if (mode === "main") {
          await page.waitForSelector("h1", { timeout: 1_000 });
          return page;
        }
      } catch {
        // The document may still be navigating; retry on the next pass.
      }
    }
    return null;
  }, timeoutMs);
}

async function findBubblePage(browser: Browser): Promise<Page | null> {
  for (const page of browser.contexts().flatMap((context) => context.pages())) {
    try {
      const mode = await page.evaluate(() => document.body?.dataset.window ?? null);
      if (mode === "bubble") return page;
    } catch {
      // Ignore a page that is still loading.
    }
  }
  return null;
}

async function connect(port: number, timeoutMs: number): Promise<Browser> {
  await waitFor(async () => {
    try {
      const response = await fetch(`http://127.0.0.1:${port}/json/version`, {
        signal: AbortSignal.timeout(1_000),
      });
      return response.ok ? true : null;
    } catch {
      return null;
    }
  }, timeoutMs);
  return chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 15_000 });
}

export async function launchNativeApp(options: { dirs?: NativeDirs } = {}): Promise<NativeApp> {
  if (!existsSync(EXE)) {
    throw new Error(
      `native binary not found at ${EXE}; run "pnpm tauri build --debug --no-bundle" first`,
    );
  }
  const dirs = options.dirs ?? (await createDirs());
  const port = await freePort();
  const child = spawn(EXE, [], {
    cwd: path.dirname(EXE),
    windowsHide: false,
    stdio: ["ignore", "pipe", "pipe"],
    env: {
      ...process.env,
      // Debug-only switches (see credentials/mod.rs and app/mod.rs): the
      // shipped release binary ignores both.
      TOKENOWL_TEST_DATA_DIR: dirs.appData,
      TOKENOWL_TEST_CREDENTIAL_SERVICE: dirs.credentialService,
      // Keep the local-usage scanner away from the developer's real home.
      // USERPROFILE itself stays untouched: WebView2 derives part of its
      // environment from it and refuses the CDP endpoint when it is moved.
      TOKENOWL_TEST_HOME: dirs.home,
      WEBVIEW2_USER_DATA_FOLDER: dirs.webview,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
    },
  });

  let output = "";
  const capture = (chunk: Buffer) => {
    if (output.length < 64 * 1024) output += chunk.toString();
  };
  child.stdout?.on("data", capture);
  child.stderr?.on("data", capture);

  let browser: Browser;
  try {
    browser = await connect(port, 30_000);
  } catch (error) {
    child.kill();
    throw new Error(`could not attach to the app's webview: ${String(error)}\n${output}`);
  }

  let page: Page;
  try {
    page = await waitForMainPage(browser, 30_000);
  } catch (error) {
    // Name what the harness actually saw: a binary built with plain
    // `tauri build --debug` still carries build.devUrl, so it waits for the
    // Vite dev server on 1420 and shows a blank window when none is running.
    // That misbuild used to surface only as a 30-second timeout.
    const observed = browser
      .contexts()
      .flatMap((context) => context.pages())
      .map((page) => page.url())
      .join(", ");
    await browser.close().catch(() => undefined);
    child.kill();
    throw new Error(`the dashboard never rendered: ${String(error)}
observed pages: ${observed || "(none)"}
build the binary with "pnpm build:native" so build.devUrl is cleared and the
embedded frontend is used instead of a Vite dev server
${output}`);
  }

  const app: NativeApp = {
    process: child,
    pid: child.pid ?? 0,
    browser,
    page,
    dirs,
    port,
    logs: () => output,
    async bubblePage(timeoutMs = 15_000) {
      const bubble = await waitFor(() => findBubblePage(browser), timeoutMs);
      await bubble.waitForSelector(".bubble", { timeout: 5_000 });
      return bubble;
    },
    windows: () => listWindows(child.pid ?? 0),
    async close({ keepDirs = false } = {}) {
      await browser.close().catch(() => undefined);
      if (child.exitCode === null) {
        child.kill();
        const exited = await Promise.race([
          new Promise<boolean>((resolve) => child.once("exit", () => resolve(true))),
          sleep(5_000).then(() => false),
        ]);
        if (!exited) {
          await execFileAsync("taskkill", ["/PID", String(child.pid), "/T", "/F"]).catch(
            () => undefined,
          );
        }
      }
      if (!keepDirs) {
        for (let attempt = 0; attempt < 6; attempt += 1) {
          try {
            await fs.rm(dirs.root, { recursive: true, force: true });
            break;
          } catch {
            await sleep(250);
          }
        }
      }
    },
  };
  return app;
}

export async function listWindows(pid: number): Promise<NativeWindow[]> {
  if (pid <= 0) return [];
  try {
    const { stdout } = await execFileAsync("powershell.exe", [
      "-NoProfile",
      "-ExecutionPolicy",
      "Bypass",
      "-File",
      WINDOW_SCRIPT,
      "-ProcessId",
      String(pid),
    ]);
    const trimmed = stdout.replace(/^\uFEFF/, "").trim();
    if (!trimmed) return [];
    const parsed = JSON.parse(trimmed) as NativeWindow | NativeWindow[];
    return Array.isArray(parsed) ? parsed : [parsed];
  } catch {
    return [];
  }
}

/** Resizes the native main window to physical pixels and returns its rect. */
export async function resizeMainWindow(
  pid: number,
  width: number,
  height: number,
): Promise<{ ok: boolean; width: number; height: number } | null> {
  try {
    const { stdout } = await execFileAsync("powershell.exe", [
      "-NoProfile",
      "-ExecutionPolicy",
      "Bypass",
      "-File",
      RESIZE_SCRIPT,
      "-ProcessId",
      String(pid),
      "-Title",
      "TokenOwl",
      "-Width",
      String(Math.round(width)),
      "-Height",
      String(Math.round(height)),
    ]);
    const trimmed = stdout.replace(/^\uFEFF/, "").trim();
    return trimmed ? (JSON.parse(trimmed) as { ok: boolean; width: number; height: number }) : null;
  } catch {
    return null;
  }
}

/**
 * Deletes every generic credential in the run's throwaway namespace. The
 * shipped service is never passed here; only `com.tokenowl.test.native.*`
 * suffixes reach this helper, so a developer's real accounts cannot be
 * touched. `cmdkey` is the OS-provided view of the same Credential Manager
 * entries the keyring crate writes; its target line is
 * `{credential_ref}.{service}`.
 */
export async function clearTestCredentials(service: string): Promise<void> {
  if (!service.startsWith("com.tokenowl.test.native.")) return;
  for (const target of await testCredentialTargets(service)) {
    await execFileAsync("cmdkey.exe", [`/delete:${target}`], {
      windowsHide: true,
    }).catch(() => undefined);
  }
}

/** Live Credential Manager entries belonging to one throwaway namespace. */
export async function testCredentialTargets(service: string): Promise<string[]> {
  if (!service.startsWith("com.tokenowl.test.native.")) return [];
  const { stdout } = await execFileAsync("cmd.exe", ["/d", "/c", "cmdkey /list"], {
    encoding: "utf8",
    windowsHide: true,
  }).catch(() => ({ stdout: "" }));
  return [...stdout.matchAll(/target=([^\s]+)/g)]
    .map((match) => match[1])
    .filter((target) => target.endsWith(`.${service}`));
}
