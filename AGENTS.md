# Repository Guidelines

## Project Overview

TokenOwl is a local-first Tauri 2 desktop monitor for AI-provider quota and usage data. The product is Windows/macOS-first: a low-distraction floating bubble opens a React dashboard showing provider accounts, quota windows, remaining values, reset timing, freshness, and errors.

The repository is an early implementation baseline. The current UI is a functional shell, while the `docs/` files describe the broader target architecture and roadmap. Treat documented future modules as design guidance, not as existing code.

## Architecture & Data Flow

- **Desktop shell:** Tauri 2 hosts a Vite + React + TypeScript frontend and a Rust library crate. `src-tauri/src/main.rs` is the thin native entry point; `src-tauri/src/lib.rs` owns Tauri setup, tray wiring, window commands, and runtime state.
- **Frontend:** `src/main.tsx` mounts `App` under `React.StrictMode`. `src/App.tsx` currently owns the dashboard/bubble shell and local UI state for expanded, form, provider name, and empty/loading/error/unsupported states. `src/App.css` contains the current visual implementation.
- **Rust domain primitives:** `src-tauri/src/provider.rs` defines provider descriptors, capabilities, authentication metadata, normalized quota snapshots, structured provider errors, OpenAI-compatible configuration, parsing, and a provider registry. `src-tauri/src/state.rs` defines account metadata, snapshot storage/views, freshness policies, position settings, refresh state, and backoff policies.
- **Current IPC:** Tauri commands in `src-tauri/src/lib.rs` cover bubble position read/write plus opening and hiding the dashboard. The frontend currently does not integrate provider data through IPC.
- **Intended flow:** scheduler/account credential resolution -> provider adapter -> raw response parser -> normalized snapshot -> validation/persistence -> Tauri event/IPC -> React state -> bubble/dashboard. Preserve raw responses at the provider boundary and expose normalized values; missing provider values remain unknown (`Option`/`null`), never fabricated as zero.
- **Failure behavior:** provider failures should be classified, retain the last known good snapshot until hard expiry, mark stale/error state, and use bounded exponential backoff. Do not freeze or blank the UI because one provider failed.
- **Security boundary:** credentials belong in the OS credential store. Persist only a credential reference and non-secret account metadata; never place API keys, tokens, cookies, or raw sensitive responses in SQLite, fixtures, source, or logs.

## Key Directories

- `src/`: React/Vite frontend entry points and current dashboard shell.
- `src-tauri/src/`: Rust application entry point plus provider and state domain primitives.
- `src-tauri/`: Cargo crate, Tauri configuration, capabilities, generated platform assets, and icons.
- `docs/`: product requirements, architecture, provider protocol, data/API design, UI guidance, security/testing/release rules, agent workflow, roadmap, and implementation status.
- `public/`: static Vite/Tauri assets.
- `dist/` and `target/`: generated build output; do not edit manually.

## Development Commands

Run commands from the repository root.

```bash
pnpm install
pnpm dev                 # Vite frontend at http://localhost:1420
pnpm build               # tsc followed by vite build
pnpm typecheck           # tsc --noEmit
pnpm lint                # currently aliases tsc --noEmit
pnpm test                # Node's built-in test runner
pnpm tauri dev           # Tauri desktop development flow
pnpm tauri build         # Tauri package/build flow
cargo fmt --check        # from src-tauri/
cargo test               # from src-tauri/
cargo clippy -- -D warnings  # from src-tauri/
```

Tauri itself is configured in `src-tauri/tauri.conf.json`: frontend development uses port `1420`, the build output is `../dist`, and the Tauri precommands are `npm run dev` and `npm run build`. The Vite server uses a strict fixed port and ignores `src-tauri` while watching.

## Code Conventions & Common Patterns

- **TypeScript:** strict mode is enabled with `noUnusedLocals`, `noUnusedParameters`, and `noFallthroughCasesInSwitch`. Use typed React props/state and keep UI state explicit. Prefer small components and accessible native controls; preserve `aria-*`, labels, keyboard focus, and visible loading/empty/error/unsupported states.
- **Rust:** edition 2024, minimum Rust 1.85. Public serialized models use Serde with explicit casing (`camelCase` for object fields, `snake_case` or `SCREAMING_SNAKE_CASE` where established by the type). Keep provider-specific raw formats at adapter/parser boundaries and use normalized domain types internally.
- **Errors:** provider errors use `ProviderErrorKind` and `ProviderResult<T>` rather than unstructured strings. Include HTTP status/retryability/retry timing when available. Do not turn unknown values into successful-looking defaults.
- **State and timing:** freshness boundaries and retry/backoff policies are explicit value objects. Keep persistence/network/credential concerns out of the state primitives; `state.rs` intentionally contains no network or secret-store code.
- **Testing patterns:** Rust domain tests use `#[cfg(test)]` modules with deterministic fixtures and boundary assertions. Provider tests cover success, partial data, malformed JSON, auth errors, rate limits, empty responses, and capability declarations.
- **Dependencies:** use the existing Tauri/React/Rust stack and local patterns. Do not introduce a second state-management or networking convention without updating the architecture and affected callers.

## Important Files

- `README.md`: document index, product invariants, implementation phases, and AI-agent reading order.
- `docs/07-AI-Agent开发执行规范.md`: required agent workflow, baseline/build/test expectations, prohibitions, diff audit, and provider/UI checklists.
- `docs/02-技术架构.md`: intended layers and data flow.
- `docs/06-安全隐私测试与发布.md`: credential, network, testing, CI, and release requirements.
- `docs/IMPLEMENTATION-STATUS.md`: current implementation state, limitations, known bugs, and technical debt.
- `docs/IMPLEMENTATION-GAP.md`: requirement-by-requirement implementation gap matrix.
- `package.json`: frontend scripts, dependencies, and pinned package manager declaration.
- `vite.config.ts`: Vite/Tauri dev server behavior and port `1420`.
- `tsconfig.json`: strict frontend compiler settings.
- `src-tauri/Cargo.toml`: Rust dependencies, lint levels, edition/version, and release profile.
- `src-tauri/tauri.conf.json`: windows, CSP, dev/build commands, and bundle settings.
- `src/App.tsx`: current dashboard shell and UI state transitions.
- `src-tauri/src/lib.rs`: Tauri commands, runtime state, tray setup, and application bootstrap.
- `src-tauri/src/provider.rs`: provider protocol/domain models, parser, registry, and fixture-style tests.
- `src-tauri/src/state.rs`: freshness, snapshot, position, refresh, backoff, and account metadata primitives/tests.

## Runtime/Tooling Preferences

- Use the repository-declared package manager: `pnpm` version `10.33.2` is specified in `package.json`. `package-lock.json` also exists from initialization; avoid changing lockfiles casually and keep the chosen install workflow consistent with the active baseline.
- Frontend runtime/tooling: Node-compatible environment, TypeScript, Vite, React 19, and the local `@tauri-apps/cli` package. Do not assume a globally installed Tauri CLI.
- Backend tooling: Rust/Cargo with edition 2024 and `rust-version = 1.85`; current development targets the Windows desktop environment, while macOS behavior remains unverified.
- Tauri security is restrictive by default: CSP is configured for self-hosted resources and no network capability should be opened without an explicit security review.

## Testing & QA

- Run frontend type/build checks with `pnpm typecheck` and `pnpm build`. `pnpm lint` currently runs the same TypeScript no-emit check; there is no separate ESLint configuration in the current repository.
- Run frontend tests with `pnpm test` (`node --test`). No frontend test files or dedicated browser-test configuration are currently present; do not claim UI coverage without adding and running it.
- Run Rust tests from `src-tauri/` with `cargo test`; current unit tests live inside `provider.rs` and `state.rs`.
- Run Rust formatting and lint checks with `cargo fmt --check` and `cargo clippy -- -D warnings`. Cargo declares Rust/clippy lints as warnings, so the strict clippy command is the useful gate.
- For provider changes, add deterministic offline fixture coverage for success, partial/missing fields, malformed data, unauthorized, rate-limited, server-error, and empty responses as applicable. Verify status mapping, field/unit/reset mapping, and retry behavior.
- For state changes, test boundary conditions: freshness transitions, stale fallback/hard expiry, backoff caps/jitter, position normalization, secret-metadata removal, mixed or missing values, and reset/timezone behavior when implemented.
- For UI changes, manually verify empty/loading/success/partial/stale/error/offline/unsupported states, keyboard access, narrow windows, high-DPI behavior, reduced motion, and long labels. The project guidance requires actual screenshots/recordings for shell interaction work and real build artifacts before release claims.
- Keep real credentials and provider account data out of tests. Use fixtures and explicit empty states. Do not claim cross-platform support until the relevant Windows/macOS flows have actually been exercised.
