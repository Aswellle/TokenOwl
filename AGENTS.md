# Repository Guidelines

## Project Overview

TokenOwl is a local-first Windows/macOS desktop monitor for AI-provider quota, balance, usage, and reset information, built on Tauri 2 with a Rust backend and React 19 webview. A small always-on-top bubble opens the main dashboard; all data stays on-device.

The implementation is in place: embedded provider manifests, a unified HTTP transport, refresh orchestration with per-account single-flight, SQLite persistence, OS keychain, a notification pipeline, and a signed updater. The numbered `docs/` (Chinese) remain the product/architecture spec but are **local-only** (`.gitignore`d): they exist on developer machines, not in the repository. Windows is the only verified platform; macOS is unverified.

## Architecture & Data Flow

- **Rust backend** (`apps/desktop/src-tauri/src/`) is layered: `domain/` (provider, state, reset, balance, history value objects), `providers/` (`builtin`, `manifest`, `registry`; `ProviderAdapter` trait + `AdapterRegistry`), `scheduler/` (priority queue with jitter/backoff), `storage/` (SQLite WAL; normalized data only — no raw response bodies, no secrets), `credentials/` (OS keychain via `keyring`; SQLite stores only `credential_ref`), `networking/` (shared HTTP client: timeouts, response-size limit, TLS/redirect policy, header redaction), `notifications/`, `telemetry/` (structured logs with secret redaction), `commands/` (Tauri handlers), `app/` (bootstrap, tray, `invoke_handler`).
- **Frontend** (`apps/desktop/src/`): `app/main.tsx` mounts the app; `features/` per area (bubble, dashboard, history, providers, settings); `lib/tauri.ts` is the single IPC bridge; shared view models in `types/`.
- **IPC boundary:** the frontend only issues semantic commands (e.g. `accounts_list`); Rust owns persistence, credentials, network access, and DTO-to-view-model conversion. Never expose raw provider payloads to the webview.
- **Target flow:** scheduler -> keyring credential -> provider adapter -> normalized `QuotaSnapshotSet` -> validation/persistence -> Tauri command/event -> `tauriBridge` -> feature state -> bubble/dashboard. Provider-specific formats stay at the adapter boundary.
- **Failure invariant:** retain the last known good snapshot, mark stale/error state, and use bounded backoff. Never replace an unknown provider value with zero or freeze the UI because one provider failed.
- **Network/CSP:** the webview has no network permission (capabilities: core, opener, process, updater only); all HTTP happens in Rust via `networking::HttpClient`. Keep the restrictive CSP; do not broaden permissions without an explicit security review.

## Monorepo Layout

- Root is a pnpm workspace (`apps/*`) with a single `pnpm-lock.yaml` and no root `package.json`.
- `apps/desktop/` — the only real package: React/Vite frontend in `src/`, Rust crate in `src-tauri/`. All JavaScript commands run from here.
- `scripts/` — icon/regeneration helpers. There is no `packages/` tree.
- `providers/builtin/*.yaml` — declarative manifests for the built-in providers (OpenAI, Anthropic, Gemini, OpenRouter, DeepSeek, Qwen, Kimi, MiniMax, and more), embedded into the binary via `include_str!` as `BUILTIN_MANIFESTS` in `src/providers/builtin/mod.rs` and parsed by `ProviderManifest::from_yaml`. Community packs load at runtime from the app-data `community-providers/` directory.
- `docs/` — **local-only** (`.gitignore`d): numbered product/architecture/security/roadmap documents (Chinese) plus status tracking. Do not assume these files exist on a fresh clone.
- `.github/workflows/` — `ci-frontend`, `ci-rust`, `ci-version` (version-consistency check), and `release` (signed updater artifacts).
- Build artifacts (`dist/`, `src-tauri/target/`, `src-tauri/gen/`, generated icons) are generated; never edit them by hand.

## Development Commands

Install from the repository root; run everything else from `apps/desktop/` (Cargo from `apps/desktop/src-tauri/`).

```bash
pnpm install                     # workspace install (root only)

cd apps/desktop
pnpm dev                         # Vite at http://localhost:1420
pnpm typecheck                   # tsc --noEmit
pnpm lint                        # aliases typecheck
pnpm build                       # tsc && vite build
pnpm test                        # vitest run (jsdom)
pnpm tauri dev                   # desktop development
pnpm tauri build                 # package build

cd src-tauri
cargo fmt --all
cargo test
cargo clippy -- -D warnings
```

- Vite: strict port `1420`, HMR port `1421` when `TAURI_DEV_HOST` is set, ignores `src-tauri`, path alias `@` -> `/src`; the Vitest config is inline in `vite.config.ts`.
- `tauri.conf.json` reads the app version from `../package.json` (single version source; `ci-version.yml` enforces consistency) and runs `pnpm dev`/`pnpm build` as precommands.
- CI runs cargo from the repo root with `--manifest-path apps/desktop/src-tauri/Cargo.toml`, and clippy with `-D warnings --all-targets --all-features`.

## Code Conventions & Common Patterns

- **TypeScript:** strict checks with `noUnusedLocals`, `noUnusedParameters`, and `noFallthroughCasesInSwitch`. React 19 + `motion/react`; state is per-feature React hooks — no external store, and do not add one without updating the architecture docs and callers.
- **Rust:** edition 2024, `rust-version = 1.85`. Clippy `pedantic` is warn with documented allows in `Cargo.toml`; CI fails on any warning. Serde casing: `camelCase` object fields, `snake_case` ordinary enums, `SCREAMING_SNAKE_CASE` protocol/status enums.
- **Domain boundaries:** keep provider-specific raw responses in adapters/parsers and expose normalized values internally. `Option`/`null` represents unavailable data; unknown is not zero. Use structured errors that preserve HTTP status, retryability, and retry timing instead of unstructured success-looking strings.
- **Secrets:** credentials live only in the OS credential store via `credentials/`; SQLite persists only a `credential_ref`. `CredentialValue`'s `Debug` output is `REDACTED` and it is not serializable. Never put API keys, tokens, cookies, raw sensitive responses, or complete secrets in source, fixtures, SQLite, or logs; `telemetry/` redacts — keep it that way. Custom endpoint/redirect handling requires explicit URL and SSRF review.
- **UI:** status must not rely on color alone; support empty/loading/success/partial/stale/error/offline/unsupported states, and preserve labels, keyboard focus, ARIA attributes, narrow-window behavior, and reduced motion.

## Important Files

- `apps/desktop/src/lib/tauri.ts`: the IPC bridge contract (semantic commands only).
- `apps/desktop/src-tauri/src/app/mod.rs`: Tauri bootstrap, tray, windows, and command registration.
- `apps/desktop/src-tauri/src/domain/provider.rs` and `domain/state.rs`: provider protocol and snapshot/freshness/backoff primitives, with embedded unit tests.
- `apps/desktop/src-tauri/src/providers/manifest/mod.rs`: `ProviderManifest` YAML schema.
- `apps/desktop/src-tauri/capabilities/default.json`: Tauri permission boundary for the `main` and `bubble` windows.
- `apps/desktop/vite.config.ts`: dev server, chunking, and Vitest configuration.
- `docs/07-AI-Agent开发执行规范.md`: agent workflow, prohibitions, provider/UI checklists, and diff self-audit.
- `docs/IMPLEMENTATION-STATUS.md` / `docs/IMPLEMENTATION-GAP.md`: verified status and requirement-by-requirement matrix.
- `docs/02-技术架构.md` / `docs/06-安全隐私测试与发布.md`: layer boundaries and security/QA requirements.

## Runtime/Tooling Preferences

- pnpm `10.33.2` (CI-pinned; `corepack enable` per README). Single lockfile: root `pnpm-lock.yaml` — there is no root `package.json` and no `package-lock.json`. CI installs with `--frozen-lockfile` from `apps/desktop/`.
- Use the local `@tauri-apps/cli` through pnpm scripts; do not assume a global Tauri CLI.
- Node 22 and Rust ~1.94 are the recorded Windows baseline. Windows is the only verified platform; macOS (Keychain, vibrancy/private API, Apple Silicon) is unverified even though `macOSPrivateApi: true` is set.

## Testing & QA

- Frontend: `pnpm test` runs Vitest (jsdom, globals, setup file `src/test/setup.ts`). Tests live in `src/test/`; `BubbleView.test.tsx`, `Dashboard.test.tsx`, and `e2e/**` are excluded in `vite.config.ts`.
- E2E: `apps/desktop/e2e/smoke.spec.ts` plus `playwright.config.ts`; `pnpm e2e` runs Playwright against the plain Vite preview with the mocked bridge — it proves the webview app, not the native shell.
- Rust: run `cargo test` from `apps/desktop/src-tauri/`. Unit tests are embedded in the `domain/` modules; integration tests use `tests/fixtures/*.json` via `tests/provider_fixtures.rs`.
- Provider changes should add offline deterministic coverage for success, partial/missing fields, malformed JSON, unauthorized, rate-limited/`retry-after`, server error, empty responses, and status/unit/reset mapping as applicable. Never use real accounts as the only test basis.
- State changes should test freshness transitions, stale fallback/hard expiry, backoff caps, position normalization, secret-metadata removal, and reset/timezone boundaries.
- Degraded-path checks should preserve the old snapshot and visible stale state across network failure, timeout, TLS/DNS failure, 429, 500, invalid JSON, and expired auth, with bounded retry/backoff.
- UI/shell changes require manual verification of state variants, keyboard access, click-vs-drag, narrow windows, high-DPI, reduced motion, long labels, and offline behavior; include screenshots/recordings for interaction work.
- Before claiming completion, follow `docs/07`: baseline -> implementation -> unit/integration tests -> build -> manual UX test -> diff review -> self-audit. Do not claim macOS or release readiness without exercising the relevant platform/build path.
