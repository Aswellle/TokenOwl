# Repository Guidelines

## Project Overview

TokenOwl is a local-first Windows/macOS-oriented Tauri 2 desktop monitor for AI-provider quota, balance, usage, and reset information. A small floating bubble opens a React dashboard for provider accounts, quota windows, remaining values, freshness, and errors.

This repository is an early implementation baseline. The current code provides a Tauri shell, a React/Vite dashboard shell, Rust provider/state primitives, tray wiring, and position IPC. The scheduler, network adapters, SQLite persistence, OS credential service, and frontend provider IPC are planned or incomplete; treat `docs/` target architecture as guidance, not existing code.

## Architecture & Data Flow

- **Desktop shell:** `src-tauri/src/main.rs` is the thin native entry point and calls `tokenowl_lib::run()`. `src-tauri/src/lib.rs` builds Tauri, owns tray behavior, managed runtime state, and window/position commands.
- **Frontend:** `index.html` -> `src/main.tsx` -> `src/App.tsx`. `main.tsx` mounts `App` under `React.StrictMode`; `App.tsx` currently owns local bubble/dashboard, form, provider-name, and empty/loading/error/unsupported state. `src/App.css` owns the shell styling and responsive/reduced-motion rules.
- **Rust domain:** `src-tauri/src/provider.rs` defines provider descriptors, capabilities, normalized quota snapshots, structured errors, OpenAI-compatible configuration/parsing, and a registry. `src-tauri/src/state.rs` defines account metadata sanitization, snapshots, freshness, position, refresh, and backoff policies.
- **Current IPC:** position read/write and dashboard open/hide commands are registered in `lib.rs`; the frontend does not yet consume provider data through IPC. Bubble position is currently runtime state, not durable persistence.
- **Target flow:** scheduler -> account/credential resolution -> provider adapter -> raw response parser -> normalized `QuotaSnapshot` -> validation/persistence -> Tauri event/IPC -> React state -> bubble/dashboard. Provider-specific formats stay at the adapter boundary.
- **Failure invariant:** retain the last known good snapshot when possible, mark stale/error state, and use bounded backoff. Never replace an unknown provider value with zero or freeze the UI because one provider failed.

## Key Directories

- `src/`: React/Vite frontend entry points and current dashboard shell.
- `src-tauri/src/`: Rust application entry point, Tauri runtime, provider domain, and state primitives.
- `src-tauri/`: Cargo crate, Tauri config, capabilities, build script, generated platform assets, and icons.
- `src-tauri/capabilities/`: Tauri permission boundary; `default.json` currently grants core/opener defaults to `main` and `bubble`, with no network permission.
- `docs/`: product requirements, target architecture, provider protocol, data model, UI, security/QA, agent workflow, roadmap, and implementation status.
- `public/`: static frontend assets.
- `dist/`, `src-tauri/target/`, and generated capability schemas under `src-tauri/gen/`: build artifacts; do not edit manually.

## Development Commands

Run frontend commands from the repository root. Run Cargo commands from `src-tauri/`.

```bash
pnpm install
pnpm dev                         # Vite at http://localhost:1420
pnpm typecheck                   # tsc --noEmit
pnpm lint                        # currently the same TypeScript check
pnpm build                       # tsc followed by vite build
pnpm test                        # Node built-in test runner
pnpm preview                     # preview the Vite build
pnpm tauri dev                   # local Tauri CLI, desktop development
pnpm tauri build                 # local Tauri CLI, package build

cd src-tauri
cargo fmt --check
cargo test
cargo clippy -- -D warnings
```

`vite.config.ts` requires port `1420`, uses HMR port `1421` when `TAURI_DEV_HOST` is set, and ignores `src-tauri`. `src-tauri/tauri.conf.json` runs `npm run dev`/`npm run build` as Tauri precommands and packages root `dist`.

## Code Conventions & Common Patterns

- **TypeScript:** strict compiler checks, `noUnusedLocals`, `noUnusedParameters`, and `noFallthroughCasesInSwitch` are enabled. Keep props and state typed, prefer small components and explicit UI states, and preserve labels, keyboard focus, ARIA attributes, narrow-window behavior, and reduced motion.
- **Rust:** edition 2024, minimum Rust 1.85. Use Serde models with established casing: `camelCase` for object fields, `snake_case` for ordinary enums, and `SCREAMING_SNAKE_CASE` for protocol/status enums.
- **Domain boundaries:** keep provider-specific raw responses in provider adapters/parsers and expose normalized values internally. `Option`/`null` represents unavailable data; unknown is not zero.
- **Errors:** use `ProviderErrorKind` and `ProviderResult<T>` in `provider.rs`; preserve HTTP status, retryability, and retry timing when available instead of returning unstructured success-looking strings.
- **State:** keep network, persistence, and credential-store concerns out of `state.rs`; its value objects model freshness, snapshots, position, refresh, and backoff.
- **Frontend state:** the current shell uses React `useState`; do not add a second state-management or networking convention without updating the architecture and callers.
- **Security:** credentials belong in the OS credential store. Persist only a credential reference and non-secret account metadata. Never put API keys, OAuth tokens, cookies, raw sensitive responses, or complete secrets in source, fixtures, SQLite, or logs. Any future custom endpoint/redirect handling requires explicit URL and SSRF review.
- **UI:** status must not rely on color alone; support empty/loading/success/partial/stale/error/offline/unsupported states and accessible native controls.

## Important Files

- `README.md`: product invariants, required document index, implementation phases, and agent reading order.
- `docs/07-AI-Agent开发执行规范.md`: baseline, implementation workflow, prohibitions, provider checklist, UI checklist, and diff self-audit.
- `docs/02-技术架构.md`: intended layers, scheduler/network/storage boundaries, and normal/failure data flows.
- `docs/06-安全隐私测试与发布.md`: credential, network, fixture, CI, offline, E2E, performance, and release requirements.
- `docs/IMPLEMENTATION-STATUS.md`: current status, limitations, known bugs, and technical debt.
- `docs/IMPLEMENTATION-GAP.md`: requirement-by-requirement implemented/missing matrix.
- `package.json`: scripts, dependencies, and declared `pnpm@10.33.2` package manager.
- `tsconfig.json` / `tsconfig.node.json`: frontend and Vite TypeScript compiler settings.
- `vite.config.ts`: React/Vite/Tauri development-server behavior.
- `src-tauri/Cargo.toml`: Rust version, dependencies, lint levels, and release profile.
- `src-tauri/tauri.conf.json`: windows, CSP, Tauri precommands, frontend output, and bundle targets.
- `src-tauri/src/lib.rs`: Tauri bootstrap, tray, runtime state, commands, and window behavior.
- `src-tauri/src/provider.rs`: provider protocol primitives, parser, registry, and provider tests.
- `src-tauri/src/state.rs`: snapshot/freshness/position/backoff primitives and state tests.

## Runtime/Tooling Preferences

- Use the repository-declared `pnpm@10.33.2` for root scripts and the local `@tauri-apps/cli`; do not assume a global Tauri CLI.
- `package-lock.json` is currently the only JavaScript lockfile, while `package.json` declares pnpm and Tauri precommands use npm. Avoid casual lockfile churn; keep the chosen install workflow consistent and validate any lockfile change.
- Frontend tooling is Node-compatible TypeScript, Vite, React 19, and Tauri 2. The recorded Windows baseline used Node 22.14.0/npm 10.33.2; do not infer cross-platform support from that baseline.
- Rust uses Cargo with edition 2024 and `rust-version = 1.85`; the recorded Windows baseline used Rust 1.94.0. macOS behavior, Keychain, menu bar, and Apple Silicon builds remain unverified.
- Tauri CSP is restrictive and self-hosted by default. Do not open network capabilities or broaden permissions without an explicit security review.
- Do not manually edit `dist`, `target`, generated schemas, or platform icon outputs.

## Testing & QA

- Frontend: `pnpm typecheck` and `pnpm build` are the meaningful current checks; `pnpm lint` aliases typecheck. `pnpm test` invokes `node --test`, but there are currently no frontend test files, browser runner, component-test setup, or coverage configuration.
- Rust: run `cargo test` from `src-tauri/`. Unit tests are embedded in `provider.rs` and `state.rs`. Provider tests use deterministic inline JSON and cover success, partial/missing values, malformed JSON, auth failure, rate limiting/retry-after, empty responses, capabilities, and registry lookup. State tests cover freshness boundaries, stale fallback/hard expiry, capped backoff/jitter, position normalization, and sensitive metadata removal.
- Provider changes should add offline deterministic coverage for success, partial/missing fields, malformed data, unauthorized, rate-limited, server-error, empty responses, status mapping, unit/reset mapping, and retry behavior as applicable. Never use real accounts as the only test basis.
- State changes should test freshness transitions, stale fallback/hard expiry, backoff caps, position normalization, secret-metadata removal, mixed/missing values, and reset/timezone boundaries when implemented.
- UI/shell changes require manual verification of state variants, keyboard access, click-vs-drag, narrow windows, high-DPI, reduced motion, long labels, and offline behavior. Shell interaction work should include screenshots/recordings; release claims require real build artifacts.
- Degraded-path checks should preserve the old snapshot and visible stale state across network failure, timeout, TLS/DNS failure, 429, 500, invalid JSON, and expired auth, with bounded retry/backoff.
- Before claiming completion, follow the documented sequence: baseline -> implementation -> unit/integration tests -> build -> manual UX test -> diff review -> self-audit. Do not claim macOS or release readiness without exercising the relevant platform/build path.
