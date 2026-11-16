import { expect, test } from "./fixtures";
import fs from "node:fs/promises";
import path from "node:path";
import { clearTestCredentials, createNativeDirs, launchNativeApp } from "./native-app";
import { appDataContains, openHistory } from "./helpers";

const PROMPT_MARKER = "PROMPT_MARKER_MUST_NEVER_BE_STORED";

function assistantLine(
  uuid: string,
  input: number,
  output: number,
  cacheRead: number,
  cacheWrite: number,
  secondsAgo: number,
): string {
  return JSON.stringify({
    type: "assistant",
    uuid,
    sessionId: "native-session",
    timestamp: new Date(Date.now() - secondsAgo * 1000).toISOString(),
    message: {
      role: "assistant",
      model: "claude-sonnet-native",
      usage: {
        input_tokens: input,
        output_tokens: output,
        cache_read_input_tokens: cacheRead,
        cache_creation_input_tokens: cacheWrite,
      },
      content: PROMPT_MARKER,
    },
  });
}

function codexLines(): string {
  const timestamp = new Date(Date.now() - 30_000).toISOString();
  return [
    JSON.stringify({
      type: "session_meta",
      timestamp,
      payload: { id: "codex-session", cwd: "C:\\private\\workspace" },
    }),
    JSON.stringify({
      type: "turn_context",
      timestamp,
      payload: { model: "gpt-5-codex" },
    }),
    JSON.stringify({
      type: "event_msg",
      timestamp,
      payload: {
        type: "token_count",
        model: "gpt-5-codex",
        info: {
          last_token_usage: { input_tokens: 300, output_tokens: 30, cached_input_tokens: 5 },
          total_token_usage: { input_tokens: 300, output_tokens: 30, cached_input_tokens: 5 },
        },
      },
    }),
    JSON.stringify({
      type: "event_msg",
      timestamp,
      payload: {
        type: "token_count",
        model: "gpt-5-codex",
        info: {
          last_token_usage: { input_tokens: 100, output_tokens: 10, cached_input_tokens: 0 },
          total_token_usage: { input_tokens: 400, output_tokens: 40, cached_input_tokens: 5 },
        },
      },
    }),
  ].join("\n");
}

/** Sum of one source's events via the real, opt-in-only scan command. */
async function scanSummary(
  page: import("@playwright/test").Page,
): Promise<Array<{ sourceId: string; newEvents: number }>> {
  return page.evaluate(() =>
    (
      window as unknown as {
        __TAURI_INTERNALS__: {
          invoke: (command: string) => Promise<Array<{ sourceId: string; newEvents: number }>>;
        };
      }
    ).__TAURI_INTERNALS__.invoke("local_usage_scan"),
  );
}

test("local usage opt-in scans both sources, resumes incrementally, and stores no prompt text", async () => {
  const dirs = await createNativeDirs();
  const claudeFile = path.join(dirs.home, ".claude", "projects", "-demo", "session.jsonl");
  const codexFile = path.join(
    dirs.home,
    ".codex",
    "sessions",
    "2026",
    "11",
    "16",
    "rollout.jsonl",
  );
  await fs.mkdir(path.dirname(claudeFile), { recursive: true });
  await fs.mkdir(path.dirname(codexFile), { recursive: true });
  await fs.writeFile(
    claudeFile,
    [
      assistantLine("claude-line-1", 1000, 200, 50, 10, 120),
      JSON.stringify({
        type: "user",
        uuid: "prompt-line",
        timestamp: new Date().toISOString(),
        message: { role: "user", content: PROMPT_MARKER },
      }),
      assistantLine("claude-line-2", 234, 34, 0, 0, 60),
      "",
    ].join("\n"),
    "utf8",
  );
  await fs.writeFile(codexFile, `${codexLines()}\n`, "utf8");

  const app = await launchNativeApp({ dirs });
  try {
    const page = app.page;
    await openHistory(page);
    const section = page.locator(".local-usage-section");
    await expect(section.getByRole("heading", { name: "本地 Agent 使用量" })).toBeVisible();

    // Both known sources are disclosed, including the Codex parser's honest
    // "not verified against a real installation" note.
    await expect(section.getByText("~/.claude/projects")).toBeVisible();
    await expect(section.getByText("~/.codex/sessions")).toBeVisible();
    await expect(section.getByText("解析器尚未在真实安装上核验")).toBeVisible();
    await expect(section).toContainText("提示词正文");

    const scanButton = section.getByRole("button", { name: "立即扫描" });
    await expect(scanButton).toBeDisabled();

    const claudeToggle = section.getByRole("checkbox", {
      name: "启用 Claude Code 本地使用量来源",
    });
    const codexToggle = section.getByRole("checkbox", {
      name: "启用 Codex CLI 本地使用量来源",
    });
    // A controlled checkbox only flips after the real command round-trips;
    // click plus an auto-waiting assertion avoids Playwright's one-shot
    // check() state probe.
    await claudeToggle.click();
    await expect(claudeToggle).toBeChecked();
    await expect(scanButton).toBeEnabled();
    await scanButton.click();
    const totals = section.locator(".local-usage__totals");
    await expect(totals.locator("dd").nth(0)).toHaveText("1,234");
    await expect(totals.locator("dd").nth(1)).toHaveText("234");
    await expect(totals.locator("dd").nth(2)).toHaveText("50");
    await expect(totals.locator("dd").nth(3)).toHaveText("10");

    // A second scan consumes no new bytes (cursor + idempotent identity).
    await scanButton.click();
    const repeats = await scanSummary(page);
    expect(repeats.find((view) => view.sourceId === "claude-code")?.newEvents).toBe(0);
    await expect(totals.locator("dd").nth(0)).toHaveText("1,234");

    // Enabling Codex scans its rollout: two per-turn deltas (300+30+5 and
    // 100+10), never the repeated cumulative totals.
    await codexToggle.click();
    await expect(codexToggle).toBeChecked();
    await scanButton.click();
    await expect(totals.locator("dd").nth(0)).toHaveText("1,634");
    await expect(totals.locator("dd").nth(1)).toHaveText("274");
    await expect(totals.locator("dd").nth(2)).toHaveText("55");

    // An append is picked up once; a re-scan after it reports nothing new.
    await fs.appendFile(claudeFile, `${assistantLine("claude-line-3", 100, 10, 0, 0, 1)}\n`);
    await scanButton.click();
    await expect(totals.locator("dd").nth(0)).toHaveText("1,734");
    const afterAppend = await scanSummary(page);
    expect(afterAppend.find((view) => view.sourceId === "claude-code")?.newEvents).toBe(0);

    // Prompt text and assistant content never reach SQLite or the DOM.
    expect(await appDataContains(app, PROMPT_MARKER)).toBe(false);
    const domLeak = await page.evaluate(
      (marker) => document.body.innerText.includes(marker),
      PROMPT_MARKER,
    );
    expect(domLeak).toBe(false);

    // Clear deletes every event; disabling both sources disables scanning.
    await section.getByRole("button", { name: "清除全部本地使用量数据" }).click();
    await expect(section.getByText("暂无本地使用量数据")).toBeVisible();
    await claudeToggle.click();
    await expect(claudeToggle).not.toBeChecked();
    await codexToggle.click();
    await expect(codexToggle).not.toBeChecked();
    await expect(scanButton).toBeDisabled();

    // Re-enabling re-reads from the start because disabling cleared cursors.
    await claudeToggle.click();
    await expect(claudeToggle).toBeChecked();
    await scanButton.click();
    await expect(totals.locator("dd").nth(0)).toHaveText("1,334");
    await expect(totals.locator("dd").nth(1)).toHaveText("244");
  } finally {
    await app.close();
    await clearTestCredentials(dirs.credentialService);
  }
});
