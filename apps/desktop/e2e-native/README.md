# Native end-to-end suite

This suite drives the **real Windows Tauri build** — not the Vite preview. It
attaches Playwright to WebView2's CDP endpoint, so the webview talks to the
actual Rust commands, SQLite database, Windows Credential Manager, shared HTTP
client, and native windows. A loopback fixture provider stands in for a
provider API; every key is a throwaway test value.

It is separate from `pnpm e2e` because it needs a GUI session, a WebView2
runtime, and a prior native build:

```powershell
cd apps/desktop
pnpm build:native                      # debug binary with the embedded frontend
pnpm e2e:native                        # runs e2e-native/*.spec.ts, one worker
pnpm e2e:native --grep "bubble"        # one scenario
```

`build:native` merges `src-tauri/tauri.native.conf.json`, which sets `build.devUrl` to null. A plain
`tauri build --debug` keeps `build.devUrl` in the binary, so the window waits
for the Vite dev server on port 1420 and shows a blank page when none is
running — the suite then times out with "the dashboard never rendered" and no
useful cause. The script makes the native run depend only on the built assets.

## Isolation

Each test spawns its own process and deletes the whole application-data tree
afterwards. Debug builds honor three test-only switches; release builds ignore
them:

| Variable | Meaning |
|---|---|
| `TOKENOWL_TEST_DATA_DIR` | absolute application-data directory (SQLite, exports, community packs) |
| `TOKENOWL_TEST_CREDENTIAL_SERVICE` | disposable keyring namespace; must start with `com.tokenowl.test.` |
| `TOKENOWL_TEST_HOME` | home directory for the read-only local-usage scanners |

The fixture also sets `WEBVIEW2_USER_DATA_FOLDER` and a per-run
`--remote-debugging-port`. `USERPROFILE` is deliberately left alone: WebView2
derives part of its environment from it and stops exposing CDP when it moves.

The release updater is compiled out of debug builds, so a native run never
fetches `latest.json` from the production release — one full suite used to
add roughly sixty phantom downloads to the release counter.

Credential cleanup is scoped to the run's namespace and executed through
`cmdkey`; the shipped service `com.tokenowl.desktop` is never passed to the
cleanup helper. No real provider key is read, written, logged, or placed in a
fixture.

## Coverage

| File | Real path exercised |
|---|---|
| `native.spec.ts` | first run/real bridge; create account, first refresh, metrics, endpoint test, key absent from DOM/SQLite |
| `rotation.spec.ts` | validate-then-commit rotation: 401 rejection keeps the old key; verified candidate is committed and used |
| `degraded.spec.ts` | 5xx, 429 with Retry-After, non-JSON 200 keep the last snapshot and name the failure; 401 endpoint classification |
| `backup-diagnostics.spec.ts` | secret-free account export, delete + import preview + key re-entry, credential cleanup, diagnostics preview/export and stale token |
| `notifications.spec.ts` | real threshold alert generation, per-row/mark-all read state, master mute and dependent-field locking, threshold save |
| `local-usage.spec.ts` | both Claude Code and Codex fixtures, opt-in gating, cursor resume, idempotent rescans, clear/disable, prompt text absent from SQLite |
| `community.spec.ts` | community pack load/skip reporting, gateway account creation, mapped usage, declared endpoints (including `reset`) |
| `persistence.spec.ts` | refresh cadence/adaptive, dock collapse, notification mute, masked proxy URL survive a process restart |
| `window-bubble.spec.ts` | 56×56 logical tool window, DPI-aware Win32 rect/style, bubble rings/aria, dashboard↔bubble handoff both ways |
| `accessibility.spec.ts` | landmarks, headings, names/labels, duplicate ids, target sizes, opaque-background contrast, focus ring, dialog focus trap, 560×480 reflow |
| `visual.spec.ts` | captures each populated view under `verify-shots/native-e2e-2026-11/` for human review |

## Boundaries

Not covered automatically: tray menu clicks, physical global-hotkey delivery,
OS toast rendering, updater download/install, multi-monitor placement, and the
macOS shell. Those remain in the manual checklist in `../e2e/SCENARIOS.md`.
The suite proves the Windows shell and real command path it exercises; it does
not turn an unverified platform into a verified one.
