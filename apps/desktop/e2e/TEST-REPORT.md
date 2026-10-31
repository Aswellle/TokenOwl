# TokenOwl E2E & Real-Device Test Report

Date: 2026-10-31
Platform: Windows 11 (win32 10.0.22631), 1920×1080, system DPI 144 (150%)
Build: Tauri 2 dev build (`tokenowl.exe`, debug) + Vite dev server; React 19 webview
Database before/after: `%APPDATA%\com.tokenowl.app\tokenowl.sqlite3` — 0 accounts, 0 providers, 0 snapshots

## 1. Executive summary

The suite now covers three layers: component/unit (Vitest), webview e2e (Playwright,
mock bridge), and the native Windows shell (computer-use plus DPI-aware OS probes).
All automated gates are green:

| Gate | Command | Result |
|------|---------|--------|
| TypeScript | `pnpm typecheck` | 0 errors |
| Unit/component | `pnpm test` | 20 files, 161 tests passed |
| E2E (webview) | `pnpm e2e` | 9 files, 109 tests passed |
| Rust format | `cargo fmt --all -- --check` | clean |
| Rust lint | `cargo clippy --all-targets --all-features -- -D warnings` | clean |
| Rust tests | `cargo test` | 322 passed |
| IPC contract | `src/test/bridge-contract.test.ts` | 5 passed |

Real-device testing found and fixed sixteen product defects across three batches,
including a **critical** credential-persistence bug that made every real account fail
its first refresh, a built-in DeepSeek balance parser gap that made a valid real key
report `InvalidResponse`, a bubble that kept rendering a deleted account (missing
`quota:updated` on the manual/first/delete refresh paths), and a native window-region
morph that ignored the OS reduced-motion switch.

## 2. What was added

### 2.1 Harness fixes

- **Vitest timeout**: the 5 s default turned parallel jsdom CPU contention into random
  failures (`SetupForm`/`NotificationSection` timed out intermittently). The timeout is
  now 20 s; it guards hangs, not machine speed.
- **Playwright port**: the fixed port 1877 landed inside a Windows reserved range
  (1822–1921) and failed with `EACCES` before any test ran. The runner now allocates a
  free ephemeral port and pins it into the inherited environment so workers and the
  web server agree.
- **Mock scenario fixtures**: `src/lib/mockScenarios.ts` adds `?scenario=` states for
  offline, slow, stale, error, first-refresh error, partial, three real windows, full,
  unknown, many accounts, long labels, syncing, no snapshots, and every failure branch
  of refresh/delete/create/LAN/preferences/history/notifications/matrix/usage/update.

### 2.2 New e2e files

| File | Focus |
|------|-------|
| `e2e/dashboard.spec.ts` | summary, filters, row actions, provider detail, empty/offline/loading, long labels |
| `e2e/settings.spec.ts` | picker groups/variants, gateway, custom format, validation, creation, LAN consent, community packs, matrix, updater |
| `e2e/history.spec.ts` | stats, trend, heatmap, keyboard, tooltip, empty, failed read |
| `e2e/notifications.spec.ts` | log, read actions, kind filter, empty, failures, preferences |
| `e2e/accessibility.spec.ts` | main landmark, headings, names, keyboard, focus trap, reduced motion |
| `e2e/viewport.spec.ts` | 480/560/800/1440 widths, long labels, 320 bubble, narrow settings |
| `e2e/bubble.spec.ts` | extended to 14 cases: 3 rings, unknown, full, stale, offline, no-window, keyboard, drag |

### 2.3 Integration/contract tests

- `src/test/bridge-contract.test.ts` parses `src/lib/tauri.ts` and the Rust sources and
  asserts every invoked command is declared and registered, every camelCase argument
  maps to a snake_case Rust parameter, and every listened event is emitted (and vice
  versa).
- `src-tauri/tests/credential_store.rs` writes a secret through one `CredentialStore`
  and reads it through a fresh one — the exact path account creation and refresh use.
  This test fails with keyring's mock backend.

## 3. Real-device session

### 3.1 Shell geometry

| Measurement | Value |
|-------------|-------|
| Screen | 1920×1080 physical, DPI 144 |
| Main window | 1162×1093 physical; x depends on bubble anchor and right-edge clamp |
| Bubble window | 84×84 physical = 56×56 logical |
| Bubble title | `TokenOwl status`; main window hidden while collapsed |
| Taskbar | no separate bubble button; main panel has one when visible |

### 3.2 Window-region morph

A DPI-aware PowerShell sampler called `GetWindowRgn`/`GetRgnBox` every ~5 ms while the
native expand ran. The region is an OS ellipse, not a CSS clip: samples grew
382 → 651 → 767 → 870 → 962 → 1043 → 1114 → 1175 → 1310 → 1365 → 1383 → 1407 →
1414 → 1419 → 1421 → 1422 px, then the region cleared (`type=0`). The step interval is
implemented as 16 ms sleeps, so the real elapsed time is longer than the nominal 380 ms;
the motion is monotonic and smooth, which is what matters visually. After a normal
collapse the hidden dashboard's region is `type=0` (cleared).

### 3.3 Credential persistence

The real-account test exposed the critical bug: with `keyring = "3"` and no platform
feature, keyring silently uses its in-memory mock. Account creation returned success,
but the refresh could not resolve the secret (`CredentialError`). The fix enables
`windows-native` / `apple-native` / `linux-native` per target. After the fix:

- The account's keychain entry was readable from a separate process (`len=6`).
- The first refresh resolved the credential and reached the provider.
- Deleting the account removed the entry (`NoEntry`).

### 3.4 Real provider account (user-provided data)

Endpoint: `http://127.0.0.1:9910/v1`; key supplied by the user; quota endpoint `/models`.
The endpoint serves `/v1/models` (200) and chat/responses, but **no `/usage`, `/quota`,
`/balance`, or billing endpoint** (all 404). The account was created and refreshed
through the real Rust HTTP client, and the result was:

- Account creation: success; credential persisted in Windows Credential Manager.
- First refresh: `InvalidResponse` — the 200 response contained no recognized quota
  fields. The UI showed “首次刷新失败，尚未获得任何额度数据…” and did **not** invent
  quota values or rings.
- The model name `workbuddy-ai/deepseek-v4.1-flash` is provider-side metadata; TokenOwl's
  account form has no model field, and the endpoint exposes no quota window for it.

The test account was deleted and the database was restored from a pre-test backup
(0 accounts/providers/snapshots). No test key remains in the repository or keychain.

### 3.5 Real DeepSeek provider run (second batch)

A second real-provider session used a valid DeepSeek API key supplied by the user
(`https://api.deepseek.com/v1`, balance endpoint `/user/balance`).

Real API facts (probed with the key, not fabricated):

- `GET /user/balance` → 200
  `{"is_available":false,"balance_infos":[{"currency":"CNY","total_balance":"-0.23",...}]}`
- `GET /v1/models` → 200 with a model catalog.

Native run against the current build:

| Step | Result |
|------|--------|
| Create account via the UI (built-in DeepSeek profile, fixed base URL) | account created, key stored in Windows Credential Manager |
| First refresh (before the parser fix) | `InvalidResponse` — the 200 response has no top-level quota fields; the generic parser could not read `balance_infos` |
| After adding `parse_balance_response` | `余额` metric: `剩余 CNY -0.23`, source `官方 API`, data fresh |
| Account health | `额度紧张` (critical) — a negative balance with no limit must not grade healthy |
| Pressure stat hint | `未上报窗口额度` (no windowed quota exists; previously it wrongly said `等待首次数据`) |
| Provider detail | balance card + capability chip `L3 · 用量/余额`; usage stats honestly show no local records |
| Bubble | critical accent, provider mark, **no rings** (a balance has no ratio), aria `额度紧张。该账户未上报按窗口计的额度。展开 TokenOwl 面板` |
| Manual refresh | balance unchanged, still fresh |
| Delete | keychain entry `NoEntry` (both account refs checked), accounts table 0 |
| Re-add on the fixed build | full flow in one build: create → key stored → first refresh → balance displayed → critical |

Screenshots: `verify-shots/e2e-native/native-deepseek-provider-detail.png`,
`native-deepseek-bubble.png`, `native-deepseek-full-flow.png`.
The account and its notification were removed afterwards; the database ended at
0 accounts / 0 providers / 0 snapshots / 0 notifications, and no key was written to any
repository file.

### 3.6 Windowed-quota ring run (third batch, local fixture provider)

Neither real endpoint exposes windowed quota, so the consumed-ratio rings were
verified natively against a **local deterministic fixture provider** (`e2e/fixture-server.cjs`,
port 8791) that reports two real windowed metrics (`used/limit` + `starts_at/ends_at`):

- `prompt-window-5h`: 420 000 / 1 000 000 tokens → 42 % consumed, 5 h span
- `prompt-window-7d`: 8 200 000 / 10 000 000 tokens → 82 % consumed, 7 d span

Native run (custom OpenAI-compatible account, LAN-consent flow, first refresh hit the
fixture with the `Authorization` header — see `fixture-server.log`):

| Step | Result |
|------|--------|
| Dashboard | both windowed metrics with `剩余/上限/已用`, live reset countdowns (`1 小时 59 分后重置`, `1 天 23 小时后重置`), source label `自定义端点`, pressure 18 %, health `需要关注` |
| Bubble aria | `需要关注。额度已用：5h 42%，7d 82%。展开 TokenOwl 面板` — two rings, shortest window outside |
| Ring pixels (DPI-aware PowerShell CopyFromScreen + angular sampling) | outer pink ring arc **165°/360** (theory 42 % × 360° = 151.2° + 2 round caps ≈ 13.6° = 164.8°); inner cyan ring arc **318°/360** (theory 82 % × 360° = 295.2° + 2 caps ≈ 21.1° = 316.3°) — both within pixel-sampling tolerance |

Evidence: `verify-shots/e2e-native/native-fixture-windowed-dashboard.png`,
`native-fixture-bubble-rings.png`, `ring-samples.csv`, `radial-scan.csv`,
`fixture-server.log`, `bubble-ring-sample.ps1`.

The fixture run also closed the ring-lens gap: the bubble draws the **consumed** share
(42 %/82 %, not the remaining 58 %/18 %) with the honest unknown/zero rules intact.

### 3.7 OS-level reduced-motion A/B (third batch)

Windows' "Show animations in Windows" (`SPI_GETCLIENTAREAANIMATION`) was toggled for
real while sampling the main window's GDI region (`GetWindowRgn` + `GetRgnBox` every
~8 ms through a real collapse/expand cycle):

| OS setting | Collapse | Expand |
|------------|----------|--------|
| Off (reduced) | 0 real region frames — window hid between two samples | 0 real region frames — instant reveal |
| On (default) | 20+ continuously changing ellipse frames (radius eases to the anchor, `region-animated.csv` t=26283–26811) | same animated reveal |

The `Off` rows were captured on the fixed build (`client_area_animation_enabled()` gates
the region-morph step count); the `On` rows double as the regression check that the
animated path still eases. Evidence: `region-reduced*.csv`, `region-animated.csv`,
`morph-hotkey-sample.ps1`.

## 4. Defects found and fixed

| # | Severity | Finding | Fix | Verification |
|---|----------|---------|-----|--------------|
| F1 | High | 0 accounts reported overall health “正常” | new `unconfigured` state + “尚未连接” copy/style | health unit test, dashboard/bubble e2e, native screenshot |
| F2 | Medium | Empty bubble aria said “该账户未上报…”, implying an account existed | separate `noAccount` / `offline` / `noWindow` copy | BubbleView test, bubble e2e, native AX tree |
| F3 | Critical | keyring mock backend: credentials never persisted; every real account failed `CredentialError` | per-target native keyring features + cross-instance credential test | real endpoint account, keychain probe, Rust test |
| F4 | Medium | First refresh failure with no snapshots had no explanation | `accountErrorEmpty` notice on row and provider view | AccountRow test, dashboard e2e, native screenshot |
| F5 | Low | First failure showed both the error notice and “点击刷新…” | suppress the generic empty hint when status is error | AccountRow test, dashboard e2e, native screenshot |
| F6 | Medium | API key input was `type=text`; CSS masked visually but the AX tree exposed the value | `type=password` | SetupSubmit test, settings e2e, native AX tree (bullets only) |
| F7 | High | Overlapping expand/collapse let an older region task overwrite the newer shape; hidden dashboard kept a 1431×1432 region | `MorphEpoch` ownership token; only the newest task writes/clears | MorphEpoch test; a normal collapse after the fix sampled `type=0` |
| F8 | Low | Deleting an account left its `providers` row (the foreign key only cascades accounts → providers) | `delete_account` now removes the row when its last referencing account goes (transactional); storage tests for the single- and shared-provider cases | sqlite counts 1 → 0 after native deletion; 2 Rust tests |
| F9 | Fixture | Browser mock has 25 profiles / 14 matrix rows; native has 31 / 32 | documented as a mock boundary; native covered by fixtures/screenshots | native settings screenshot |
| F10 | Tool limit | WebView2 `AXPress` does not dispatch DOM clicks; CUA capture of the 84×84 bubble fails `invalid_target` | keyboard focus + Enter for interaction; DPI-aware PowerShell for bubble pixels | session notes |
| F11 | Harness | Vitest 5 s timeouts flaked under parallel load; e2e fixed port hit a Windows reserved range | 20 s timeout; dynamic pinned port | full suites |
| F12 | Medium | History failure = endless skeleton; notifications failure = false empty inbox; matrix failure = “0 providers” | explicit failure states with retry; mark-read failure alert | unit tests, e2e, native |
| F13 | Critical | Built-in DeepSeek balance endpoint: the generic parser cannot read DeepSeek's `balance_infos` envelope, so a valid real key reported `InvalidResponse` and no data | dedicated `parse_balance_response` for balance endpoints (falls back to the generic path); three parser tests | real-key native run showing `CNY -0.23`; parser tests |
| F14 | Medium | A negative balance with no limit graded the account healthy | `account_data_grade` returns Critical for any trusted negative remaining | commands test `a_negative_balance_without_a_limit_is_critical`; native `额度紧张` |
| F15 | Low | Built-in providers (DeepSeek etc.) were labeled `自定义端点` | `is_builtin_provider_id` + adapter source override to `OfficialApi` | builtin test; native label `官方 API` |
| F16 | Low | A balance-only account made the pressure stat say `等待首次数据` although data existed | new `noWindow` hint `未上报窗口额度` when snapshots exist but no ratio | e2e `?scenario=balance-only`; native screenshot |
| F17 | Medium | The bubble is a separate webview and only heard `quota:updated` from the scheduler/hotkey paths — manual refresh, first refresh, and account deletion left it stale (it kept rendering a deleted account's rings, or no account after a create) | `refresh_accounts` emits `quota:updated` when a refresh handle is present; `accounts_delete` emits after a successful delete (unit payload, no TS change) | native: bubble aria flipped `等待添加第一个供应商` → `5h 42%，7d 82%` immediately after create, and back after delete |
| F18 | Medium | The native window-region morph animated ~300–380 ms even with the OS "Show animations" accessibility switch off (the webview honored `prefers-reduced-motion`, the shell did not) | `client_area_animation_enabled()` (`SPI_GETCLIENTAREAANIMATION`) collapses the morph to a single step when the switch is off | OS-level A/B: 0 real region frames with animations off, 20+ frames with them on |
| F19 | Low | The titlebar close light is labeled `关闭面板（回到 Bubble）` but invoked `hide_dashboard` — a plain hide with no visible way back (only tray/hotkey) | the close light now performs the same collapse-to-bubble handoff as the header pill (preview branch included); Dashboard test pins it | Dashboard test `close light collapses to the bubble instead of plain hiding`; native collapse after the fix showed the bubble |

## 5. Unresolved / limits

1. **Real windowed-quota rings — closed by fixture**: no real endpoint available for
   this session exposes windowed quota (the user's local gateway has no quota API;
   DeepSeek reports a currency balance only), so the ring verification used the local
   deterministic fixture provider of section 3.6: dashboard metrics, reset countdowns,
   source labels, the two-ring bubble, and pixel-sampled consumed ratios (165°/318°)
   are all natively verified. What remains fixture-only is inherent: no real provider
   account with windowed quota was monitored for days.
2. **Mock catalog drift** (F9): the dev mock is a UI fixture, not a copy of the native
   manifest catalog.
3. **macOS native shell**: runtime-unverified (no macOS host). Compile-level coverage
   is wired in CI — `release.yml` builds `darwin-aarch64` and `darwin-x86_64` on
   `macos-latest` runners. A local `cargo check --target aarch64-apple-darwin` from
   Windows compiles the pure-Rust tree but stops at cc-rs build scripts (no apple
   cross C toolchain) — a toolchain limit, not a code error. Every platform-specific
   change of this session is `#[cfg(windows)]`-gated (`client_area_animation_enabled`,
   the region morph) or platform-neutral.
4. **Multi-monitor and 100%/200% DPI**: this machine has exactly one display
   (1920×1080 at 150 %), so every flow was verified single-monitor; the Rust geometry
   keeps unit coverage for monitor origins, but multi-monitor runtime behavior remains
   hardware-blocked.
5. **OS-level reduced-motion — closed for Windows**: the real accessibility switch was
   toggled and the shell's region morph verified to honor it (section 3.7). The
   webview side (`prefers-reduced-motion` → motion/react) is covered by e2e emulation.
6. **CUA capture of the bubble**: the tool refuses the 84×84 tool window; bubble pixels
   were captured with a DPI-aware PowerShell screenshot instead.
7. **Long-duration stability** (24 h) and real updater download are out of scope.

## 6. Reproduction

```bash
cd apps/desktop
pnpm typecheck
pnpm test
pnpm e2e
cd src-tauri
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

Scenario preview URLs: `/?scenario=offline`, `/?scenario=three-windows`,
`/?window=bubble&scenario=full`, `/?scenario=error-no-snapshots`,
`/?scenario=history-fail`, `/?scenario=notifications-mark-fail`, etc.

Native verification used `pnpm dev --host 127.0.0.1` plus the debug `tokenowl.exe`,
computer-use for keyboard/observation, and DPI-aware PowerShell for window-region and
screenshot measurements. The test database was backed up before the real-account run
and restored afterward.

## 7. Files changed (summary)

- Fifth batch (quota kinds + official-source corrections): manifests
  `openrouter/siliconflow/kimi/openai/qwen.yaml` (path fixes, role/name/metrics
  mappings, honest capability levels), `src-tauri/src/domain/provider.rs`
  (`FieldMapping.role/name`, `FieldMappings.metrics`, DeepSeek
  granted/topped-up split, parser tests), `src-tauri/src/providers/manifest/mod.rs`
  (metrics schema + validation), `src-tauri/src/providers/builtin/anthropic.rs`
  (official ratelimit header prefixes with per-prefix units),
  `src/features/dashboard/components/QuotaMetric.tsx` (window-type/span chips,
  unit-aware formatting), `src/features/dashboard/components/ProviderView.tsx`
  (windowed vs metered sections), `src/lib/format.ts` (percent/token formats),
  `src/lib/mockScenarios.ts` (`kinds` scenario), `src/test/QuotaMetric.test.tsx`,
  `e2e/dashboard.spec.ts` (kinds case), mock matrix/profile sync.
- Fourth batch (bubble states + morph): `e2e/bubble.spec.ts` (+8 state/motion cases),
  `src/test/BubbleView.test.tsx` (+8 centre-mark/morph-handoff cases with captured
  event listeners), `src/test/bridge-contract.test.ts` (morph anchor payload
  contract), `src/features/bubble/components/QuotaRings.tsx`
  (`data-morph-phase` observation attribute), `src-tauri/src/app/mod.rs`
  (`morph_step_count` extraction + `region_radius_at`/step-count unit tests).
- New e2e: `dashboard.spec.ts`, `settings.spec.ts`, `history.spec.ts`,
  `notifications.spec.ts`, `accessibility.spec.ts`, `viewport.spec.ts`; extended
  `bubble.spec.ts`, `SCENARIOS.md`.
- New tests: `src/test/bridge-contract.test.ts`, `src-tauri/tests/credential_store.rs`.
- New fixture: `src/lib/mockScenarios.ts`; `src/lib/mock.ts` wiring.
- Third batch: `e2e/fixture-server.cjs` (windowed-quota fixture provider),
  `src-tauri/tests/fixtures/openai-windowed.json` +
  `tests/provider_fixtures.rs` (window-span parsing),
  `src-tauri/src/storage/mod.rs` (provider row cleanup on account deletion),
  `src-tauri/src/commands/mod.rs` (`quota:updated` on manual/first refresh and delete),
  `src-tauri/src/app/mod.rs` (`client_area_animation_enabled` reduced-motion gate),
  `src/features/dashboard/App.tsx` + `src/test/Dashboard.test.tsx`
  (close light collapses to the bubble),
  `src-tauri/src/domain/provider.rs` (`starts_at`/`ends_at` window fields in the
  lenient parser).
- Earlier-batch fixes: `src-tauri/Cargo.toml` (keyring features),
  `src-tauri/src/app/mod.rs` (morph epoch), `src-tauri/src/domain/provider.rs`
  (balance envelope parser), `src-tauri/src/providers/builtin/{mod.rs,openai.rs}`
  (built-in source labeling), `src-tauri/src/commands/mod.rs` (negative-balance
  grading), `src/lib/format.ts` (unconfigured health), `src/features/...`
  (empty/failure states, API-key masking, no-window pressure hint),
  `src/styles/{App.css,copy.ts}`.
- Harness: `playwright.config.ts`, `vite.config.ts`,
  `e2e/accessibility.spec.ts` (patient first-mount wait),
  `e2e/global-setup.ts` (dev-server warmup: Vite's on-demand re-optimization
  reload now lands inside setup instead of inside the suite's first spec).
- Verification tooling (untracked, `verify-shots/e2e-native/`):
  `bubble-ring-sample.ps1`, `morph-hotkey-sample.ps1`, `region-sample.ps1`,
  `fixture-server.log`, `ring-samples.csv`, `radial-scan.csv`,
  `region-reduced*.csv`, `region-animated.csv`, native screenshots
  (`native-empty-bubble-owl.png`, `native-configured-bubble-rings.png`,
  `ring-samples-smoke.csv`).
