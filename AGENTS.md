# Repository Guidelines

## Project Overview

TokenOwl is a local-first Windows/macOS desktop monitor for AI-provider quota, balance, usage, and reset information, built on Tauri 2 with a Rust backend and React 19 webview. A small always-on-top bubble opens the main dashboard; all data stays on-device.

The implementation is in place: embedded provider manifests, a unified HTTP transport, refresh orchestration with per-account single-flight, SQLite persistence, OS keychain, a notification pipeline, and a signed updater. The numbered `docs/` (Chinese) remain the product/architecture spec but are **local-only** (`.gitignore`d): they exist on developer machines, not in the repository. Windows is the only verified platform; macOS is unverified.

## Architecture & Data Flow

- **Rust backend** (`apps/desktop/src-tauri/src/`) is layered: `domain/` (provider, state, reset, balance, history value objects), `providers/` (`builtin`, `manifest`, `community`; `ProviderAdapter` trait + `AdapterRegistry`), `scheduler/` (per-account single-flight refresh; bounded waves of `REFRESH_WAVE_SIZE` live in `commands/accounts.rs`), `storage/` (SQLite WAL; normalized data only — no raw response bodies, no secrets), `credentials/` (OS keychain via `keyring` with per-platform native backends; SQLite stores only `credential_ref`), `networking/` (shared HTTP client: timeouts, response-size limit, TLS/redirect policy, header redaction, optional explicit proxy from settings), `notifications/`, `telemetry/` (structured logs with secret redaction), `commands/` (Tauri handlers, split into `accounts`/`backup`/`diagnostics`/`exports`/`history`/`lan`/`manifest`/`notifications`/`refresh`/`settings`/`usage` submodules that `mod.rs` glob re-exports so `commands::x` paths stay stable), `app/` (bootstrap, tray, windows, bubble↔dashboard morph via `SetWindowRgn`, `invoke_handler`).
- **Frontend** (`apps/desktop/src/`): `app/main.tsx` mounts the app; `features/` per area (bubble, dashboard, history, providers, settings); `lib/tauri.ts` is the single IPC bridge; shared view models in `types/`.
- **IPC boundary:** the frontend only issues semantic commands (e.g. `accounts_list`); Rust owns persistence, credentials, network access, and DTO-to-view-model conversion. Never expose raw provider payloads to the webview.
- **Local usage sources** (`src-tauri/src/usage/`, commands in `commands/usage.rs`): a data domain deliberately separate from the remote quota pipeline. Sources are opt-in (`local_usage_sources` setting), read-only, restricted to known directories (`~/.claude/projects`, `~/.codex/sessions`), and persisted into `local_usage_events` (migration 0005) whose `(source_id, source_event_id)` unique pair makes scans idempotent. Per-file cursors live in the `local_usage_cursors` setting and store a hash of the path plus head/anchor fingerprints — never a filesystem path; appends resume, rewrites restart, and a half-written trailing line waits for its newline. Claude Code dedupes on its per-line `uuid`. Codex uses `last_token_usage` as the per-turn delta (summing `total_token_usage` over-counts a session), differences cumulative-only streams, and reads the model from `turn_context`; it remains fixture-tested but not verified against a real installation, and the UI says so. Prompt bodies, replies, source snippets, and working-directory paths are never collected; cost is always unknown (no price table). Do not merge local usage numbers into quota displays.
- **Target flow:** scheduler -> keyring credential -> provider adapter -> normalized `QuotaSnapshotSet` -> validation/persistence -> Tauri command/event -> `tauriBridge` -> feature state -> bubble/dashboard. Provider-specific formats stay at the adapter boundary.
- **Failure invariant:** retain the last known good snapshot, mark stale/error state, and use bounded backoff. Never replace an unknown provider value with zero or freeze the UI because one provider failed.
- **Network/CSP:** the webview has no network permission (capabilities: core, opener, process, updater only); all HTTP happens in Rust via `networking::HttpClient`. Keep the restrictive CSP; do not broaden permissions without an explicit security review.
- **The updater is release-only and low-frequency.** `updater_runtime_enabled` registers the updater plugin only in release builds; debug/test binaries have no updater at all (check/install commands answer with a clear error), so a native test run can never heartbeat the production release endpoint — one check is one counted GitHub download. In release builds, probes run in a dedicated task: one ~30 s after startup, then one per day (`UPDATE_CHECK_INTERVAL` in `app/mod.rs`), never on the refresh loop, whose sleep can shrink to the 60 s floor. `TOKENOWL_UPDATER_DRILL` stays the release-QA path.
- **Window lifecycle:** the app is tray-resident — a close request on the dashboard hides the window instead of destroying it (Alt+F4 must not orphan the tray). The bubble is a 56×56 tool window sized by `BUBBLE_LOGICAL_SIZE` in `app/mod.rs` (mirrored in `tauri.conf.json`; change them together). The bubble↔dashboard morph is an OS window-region animation owned by Rust with a `MorphEpoch` guard; the webviews only receive anchor events.
- **Wire formats:** adapter dispatch goes through the `WireFormat` enum (`domain/provider.rs`) and the config's persisted `api_format` (string; `None` = OpenAI-compatible, legacy rows fall back to the id prefix). Adding a fourth protocol is a compile error at the scheduler match, not a string edit in six places.
- **Capability levels:** the displayed L1–L4 comes from `ProviderManifest::effective_capability_level` — the declared level clamped to what the declared endpoints support per `docs/09`. Never advertise a level the endpoints cannot deliver; a guard test enforces this for built-ins.
- **Community manifests are data, not plugins.** `schemaVersion` (currently 1; absent means 1, newer versions are refused), `auth: [api_key]` only, and unknown-field rejection are enforced at load; the allowed field set contains no code, script, library, or redirect key. Mapping bounds cover dot-path depth/segments, units, currency/name lengths, the metrics count, and a ±1e15 numeric ceiling in the parsers. The provider picker shows a community pack's source file, schema version, and last request status. The offline playground (`manifest_playground`) validates a pasted pack and runs its mapping against a pasted sample; it never touches the network, credentials, or SQLite.
- **Refresh scheduling is per account.** `BackgroundPlan` (`commands/refresh.rs`) combines the provider's `default_refresh`, the user's 0/1/5/10/15/30-minute cadence ceiling, failure backoff, and the opt-in adaptive policy. Adaptive halves at most for an imminent reset or ≤10% remaining with activity, doubles at most for a 2-hour-idle account, and never crosses the bounds. Manual refresh bypasses the due plan as an explicit user action; provider `Retry-After` still flows through the refresh path, and the background plan always honors the local failure backoff.
- **Backups and diagnostics are secret-free.** `accounts_export_config` writes provider id, label, base URL, wire format, and endpoints only; import previews without creating and each row re-enters a key through the normal create path. `diagnostics_preview`/`diagnostics_export` exclude keys, credential refs, labels, paths, headers, bodies, proxy URLs, queries, and custom endpoint hosts; export requires the preview token and the token hashes only the timestamp-free document. `account_endpoint_test` reports one stable outcome/status/latency per declared endpoint (paths deduplicated) and no body.
- **`lastVerifiedAt` means a real API verification date** and is validated as a non-future `YYYY-MM-DD`. Built-ins do not claim one; the README shows “API 核验 = 未声明” alongside “契约覆盖 = 已覆盖” for the offline conformance suite instead of inventing dates. Local last-success times come from `RefreshDiagnostics.lastSuccessAt`.
- **README provider table is generated.** The table between `<!-- providers:begin -->` / `<!-- providers:end -->` markers is owned by `tests/readme_sync.rs`: regenerate with `TOKENOWL_UPDATE_README=1 cargo test --test readme_sync`, never by hand — the drift test fails CI otherwise. Model-only providers answer their refresh through the models probe (`providers/builtin/openai.rs`); a declared `reset` endpoint is a real request with a declared timestamp format (`rfc3339`/`unix_seconds`/`unix_millis`, never inferred).
- **History retention covers the UI window.** `HISTORY_RETENTION_SECS` (365 days) must stay ≥ the dashboard's 182-day heatmap; the boundary test pins the exact cutoff. Account views carry `RefreshDiagnostics` (stable code, HTTP status, endpoint kind, attempt/success times) — keep the DTO free of URLs, headers, and bodies.

## Monorepo Layout

- Root is a pnpm workspace (`apps/*`) with a single `pnpm-lock.yaml` and no root `package.json`.
- `apps/desktop/` — the only real package: React/Vite frontend in `src/`, Rust crate in `src-tauri/`. All JavaScript commands run from here.
- `scripts/` — icon/regeneration helpers. There is no `packages/` tree.
- `providers/builtin/*.yaml` — declarative manifests for the built-in providers (OpenAI, Anthropic, Gemini, OpenRouter, DeepSeek, Qwen, Kimi, MiniMax, and more), embedded into the binary via `include_str!` as `BUILTIN_MANIFESTS` in `src/providers/builtin/mod.rs` and parsed by `ProviderManifest::from_yaml`. Community packs load at runtime from the app-data `community-providers/` directory.
- `docs/` — **local-only** (`.gitignore`d): numbered product/architecture/security/roadmap documents (Chinese) plus status tracking. Do not assume these files exist on a fresh clone.
- `.github/workflows/` — `ci-frontend`, `ci-rust` (incl. a pinned-1.90 MSRV `cargo check` job — keep it in sync with `rust-version` — and clippy/fmt pinned to `LINT_TOOLCHAIN` in that file), `ci-version` (version-consistency check), `audit` (weekly RustSec + pnpm advisory sweep, also workflow_dispatch), and `release` (signed updater artifacts plus a native smoke that launches the freshly built binary on each target).
- `apps/desktop/src-tauri/.cargo/audit.toml` — cargo-audit ignore list; each entry documents why an advisory is unreachable or upstream-blocked. Re-evaluate whenever `tauri`, `plist`, or `tauri-plugin-updater` release.
- `pnpm-workspace.yaml` — workspace packages **and** the only pnpm override (katex). Overrides must live here: a `pnpm` block in `apps/desktop/package.json` is silently ignored in a workspace.
- Build artifacts (`dist/`, `src-tauri/target/`, `src-tauri/gen/`, generated icons) are generated; never edit them by hand.

## Development Commands

Install from the repository root; run everything else from `apps/desktop/` (Cargo from `apps/desktop/src-tauri/`).

```bash
pnpm install                     # workspace install (root only)

cd apps/desktop
pnpm dev                         # Vite; tauri passes --host 127.0.0.1 explicitly
pnpm typecheck                   # tsc --noEmit
pnpm lint                        # aliases typecheck
pnpm build                       # tsc && vite build
pnpm test                        # vitest run (jsdom)
pnpm e2e                         # playwright against the vite preview (mock bridge)
pnpm e2e:native                  # real Tauri build over WebView2 CDP (Windows, manual)
pnpm tauri dev                   # desktop development
pnpm tauri build                 # package build

cd src-tauri
cargo fmt --all
cargo test
cargo clippy -- -D warnings
```

- Vite: strict port `1420`, HMR port `1421` when `TAURI_DEV_HOST` is set, ignores `src-tauri`, path alias `@` -> `/src`; the Vitest config is inline in `vite.config.ts`.
- **Dev URLs are pinned to `127.0.0.1`, never `localhost`:** Node 17+ resolves `localhost` in an arbitrary stack order, so vite may bind only `[::1]` while the tauri shell dials IPv4 and hangs in SYN_SENT (`tauri.conf.json`'s `devUrl`/`beforeDevCommand` carry the pin — keep it).
- **After `pnpm install`, warm the dev server once before suspecting code:** fresh installs invalidate vite's optimizeDeps cache, and a cold first load can hang e2e's `global-setup` on `networkidle` or slow the first specs into false failures on Windows. One real page load (curl + a short wait) fixes it.
- `tauri.conf.json` reads the app version from `../package.json` (single version source; `ci-version.yml` enforces consistency) and runs `pnpm dev`/`pnpm build` as precommands.
- CI runs cargo from the repo root with `--manifest-path apps/desktop/src-tauri/Cargo.toml`, and clippy with `-D warnings --all-targets --all-features`.
- **Run clippy/fmt with CI's pinned toolchain, not whatever `stable` is locally:** `ci-rust.yml` pins both gates to `LINT_TOOLCHAIN` (currently `1.100.0`) because stable adds pedantic lints and reformats between releases — code green on an older local stable can be red in CI with no change (this is how `assert_is_empty` first surfaced). Before pushing Rust changes run `cargo +1.100.0 clippy --all-targets --all-features -- -D warnings` and `cargo +1.100.0 fmt --all -- --check`; bumping the pin is a deliberate change that reformats and re-lints in the same commit.

## Code Conventions & Common Patterns

- **TypeScript:** strict checks with `noUnusedLocals`, `noUnusedParameters`, and `noFallthroughCasesInSwitch`. React 19 + `motion/react`; state is per-feature React hooks — no external store, and do not add one without updating the architecture docs and callers.
- **Rust:** edition 2024, `rust-version = 1.90` (verified by a pinned CI job; bump both together when a dependency forces it). Clippy `pedantic` is warn with documented allows in `Cargo.toml`; CI fails on any warning. Serde casing: `camelCase` object fields, `snake_case` ordinary enums, `SCREAMING_SNAKE_CASE` protocol/status enums.
- **Domain boundaries:** keep provider-specific raw responses in adapters/parsers and expose normalized values internally. `Option`/`null` represents unavailable data; unknown is not zero. Use structured errors that preserve HTTP status, retryability, and retry timing instead of unstructured success-looking strings — IPC errors serialize as `{ code, message }` with stable codes the UI branches on.
- **Storage split:** the settings table is a plain key-value store for UI preferences (`dock_collapsed`, `bubble_position`, `lan_trust_targets`, `local_usage_sources`, `proxy_routing`, notification prefs); anything secret goes to the keyring (the proxy URL rides `transport/proxy` because it may embed credentials). A missing or malformed settings value must mean the safe default (e.g. dock expanded, domestic direct / international system proxy), never a hidden control.
- **Proxy routing is per region and live.** `networking::ProxyState` holds the policy (`direct` / `system` / `custom` per class) and the custom URL; every adapter shares one state, so `proxy_set` and `proxy_routing_set` apply to the next request with no restart. `ProxyRoute::System` keeps reqwest's platform detection (Windows Internet Settings); `Direct` calls `no_proxy()`; `Custom` resolves the stored URL per request. Region comes from the account config (`ProviderRegion::China` → domestic, everything else international). All three routes are built in `HttpClient::with_state`; adapters never hold a `reqwest::Client` directly but ask for `client.route(&config.region)`.
- **Key rotation is validate-then-commit** (`rotate_account_credential`): the candidate key is written under a one-time `…/candidate/{nonce}` keyring reference and probed through the account's own adapter; the stable reference is overwritten only on a verified verdict. Every other outcome (`rejected`, `validation_indeterminate`, `storage_failed`) leaves the old key in place. The frontend closes the key dialog only on `validated_and_saved`.
- **Secrets:** credentials live only in the OS credential store via `credentials/`; SQLite persists only a `credential_ref`. `CredentialValue`'s `Debug` output is `REDACTED` and it is not serializable. Never put API keys, tokens, cookies, raw sensitive responses, or complete secrets in source, fixtures, SQLite, or logs; `telemetry/` redacts — keep it that way. Custom endpoint/redirect handling requires explicit URL and SSRF review.
- **UI:** status must not rely on color alone; support empty/loading/success/partial/stale/error/offline/unsupported states, and preserve labels, keyboard focus, ARIA attributes, narrow-window behavior, and reduced motion.

## Important Files

- `apps/desktop/src/lib/tauri.ts`: the IPC bridge contract (semantic commands only) plus `listenWithRetry` for native event listeners.
- `apps/desktop/src-tauri/src/app/mod.rs`: Tauri bootstrap, tray, windows, close→hide interception, morph/`MorphEpoch`, and the `invoke_handler` list — a new command must be added there (the registration guard test verifies both directions).
- `apps/desktop/src-tauri/src/commands/{accounts,backup,diagnostics,history,lan,manifest,notifications,refresh,settings,usage}.rs`: the command surface, split by domain; `exports.rs` owns the bounded export directory, and shared runtime/`CommandError` stay in `commands/mod.rs`.
- `apps/desktop/src-tauri/tests/command_registration.rs`: parses the command sources against `invoke_handler` — if it fails after touching `commands/`, the handler list or the file layout drifted.
- `apps/desktop/src-tauri/src/domain/provider.rs` and `domain/state.rs`: provider protocol, `WireFormat`, snapshot/freshness/backoff primitives, with embedded unit tests.
- `apps/desktop/src-tauri/src/providers/manifest/mod.rs`: `ProviderManifest` YAML schema and the capability-level clamp.
- `apps/desktop/src-tauri/capabilities/default.json`: Tauri permission boundary for the `main` and `bubble` windows.
- `apps/desktop/AUDIT.md`: the tracked multi-dimensional code review (2026-11) with the dependency-audit round (§11) — read before touching audit ignore lists or dependency versions.
- `apps/desktop/e2e/SCENARIOS.md`: the interaction matrix and the three-layer verification boundary (unit / preview e2e / native manual).
- `apps/desktop/vite.config.ts`: dev server, chunking, and Vitest configuration.
- `docs/07-AI-Agent开发执行规范.md`: agent workflow, prohibitions, provider/UI checklists, and diff self-audit.
- `docs/IMPLEMENTATION-STATUS.md` / `docs/IMPLEMENTATION-GAP.md`: verified status and requirement-by-requirement matrix.
- `docs/02-技术架构.md` / `docs/06-安全隐私测试与发布.md`: layer boundaries and security/QA requirements.

## Runtime/Tooling Preferences

- pnpm `10.33.2` (CI-pinned; `corepack enable` per README). Single lockfile: root `pnpm-lock.yaml` — there is no root `package.json` and no `package-lock.json`. CI installs with `--frozen-lockfile` from `apps/desktop/`.
- Use the local `@tauri-apps/cli` through pnpm scripts; do not assume a global Tauri CLI.
- Node 22 and Rust ~1.94 are the recorded Windows baseline. Windows is the only verified platform; macOS (Keychain, vibrancy/private API, Apple Silicon) is unverified even though `macOSPrivateApi: true` is set.
- On this machine SSH over port 22 to GitHub is intermittently blocked mid-handshake; if `git push` fails with an access-rights error while `ssh -T git@github.com` succeeds, push over HTTPS (`git push https://github.com/Aswellle/TokenOwl.git main`, gh credential helper) instead of fighting the port.

## Testing & QA

- Frontend: `pnpm test` runs Vitest (jsdom, globals, setup file `src/test/setup.ts`). Tests live in `src/test/`; `BubbleView.test.tsx`, `Dashboard.test.tsx`, and `e2e/**` are excluded in `vite.config.ts`. Tests that `vi.mock` the `../lib/tauri` module must re-export every new helper the components use (e.g. `listenWithRetry`), or the mocked module leaves it undefined.
- E2E: `pnpm e2e` runs Playwright over layered suites (dashboard, settings, dock, bubble rings/morph, viewport, accessibility, flows) against the plain Vite preview with the mocked bridge — it proves the webview app, not the native shell. The port is allocated dynamically (`E2E_PORT`), never hardcoded. Native-only regressions (window stack, morph, tray) have slipped past this suite before; UI/shell changes still need the native pass below.
- Native E2E: `pnpm e2e:native` (config `playwright.native.config.ts`) attaches Playwright to WebView2 CDP in the real debug Tauri build, with a loopback provider, real IPC/SQLite/Windows Credential Manager, real windows via Win32 probes, and per-test isolated data + keyring namespaces. Build the binary first with `pnpm build:native` (a plain `tauri build --debug` keeps `build.devUrl`, so the suite would wait for the Vite dev server on 1420 and time out with a blank window); see `apps/desktop/e2e-native/README.md`. It requires a GUI Windows session, so CI does not run it. The suite covers creation/refresh, rotation, degraded paths, backup/import, diagnostics, notifications, local usage, community packs, restart persistence, the bubble handoff, accessibility targets/contrast, and visual review captures. It does not cover tray clicks, physical hotkeys, OS toast rendering, updater execution, multi-monitor, or macOS.
- Rust: run `cargo test` from `apps/desktop/src-tauri/`. Unit tests are embedded in `domain/`, `commands/*`, and `providers/*` modules (tests in `commands/mod.rs` reach submodule items through `pub(super)`); integration tests live in `tests/` (`provider_fixtures.rs` with `tests/fixtures/*.json`, `endpoint_diagnostics.rs`, `provider_conformance.rs` with the community fixture, plus `command_registration.rs` guarding the invoke-handler contract). Keyring-touching fixtures use disposable `com.tokenowl.test.*` service namespaces and delete their entries on drop — the shipped `com.tokenowl.desktop` namespace is never written by tests, and parallel same-ref writes (the documented Windows transient-missing race) are avoided by construction.
- Provider changes should add offline deterministic coverage for success, partial/missing fields, malformed JSON, unauthorized, rate-limited/`retry-after`, server error, empty responses, and status/unit/reset mapping as applicable. Never use real accounts as the only test basis.
- State changes should test freshness transitions, stale fallback/hard expiry, backoff caps, position normalization, secret-metadata removal, and reset/timezone boundaries.
- Degraded-path checks should preserve the old snapshot and visible stale state across network failure, timeout, TLS/DNS failure, 429, 500, invalid JSON, and expired auth, with bounded retry/backoff.
- UI/shell changes require manual verification of state variants, keyboard access, click-vs-drag, narrow windows, high-DPI, reduced motion, long labels, and offline behavior; include screenshots/recordings for interaction work.
- Before claiming completion, follow `docs/07`: baseline -> implementation -> unit/integration tests -> build -> manual UX test -> diff review -> self-audit. Do not claim macOS or release readiness without exercising the relevant platform/build path.
