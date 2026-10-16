# Contributing to TokenOwl

Thank you for considering a contribution! TokenOwl is a local-first desktop app, so most contributions fall into a few well-trodden paths.

## Ways to contribute

- **New provider manifests** — the highest-leverage contribution. Most providers are a single YAML file plus test fixtures; no core code changes required (see below).
- **Bug fixes** — anything from snapshot math to UI state handling. Please include a failing test first when practical.
- **UI/UX polish** — the design system is token-based; state handling (empty/loading/stale/error) and accessibility are non-negotiable.
- **Documentation** — README and Rustdoc are the public documentation; code identifiers are English, README prose English.

## Getting started

```bash
git clone https://github.com/Aswellle/TokenOwl.git
cd TokenOwl
corepack enable
pnpm install
```

Run everything from `apps/desktop/` (Cargo from `apps/desktop/src-tauri/`):

```bash
pnpm dev                # browser UI preview (mock bridge)
pnpm tauri dev          # full desktop app
pnpm typecheck && pnpm test
cargo test              # from src-tauri/
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all
```

CI enforces all of the above plus a version-consistency check (`package.json` is the single version source).

## Adding a provider manifest

1. Copy the closest existing manifest in `providers/builtin/` and adjust id, base URL, endpoints, and the honest capability level (`L1`–`L4`, see the `capability:` block of any existing manifest). **Grade what the API actually exposes** — if a provider has no public quota endpoint, it is L2 health-only, never an invented limit.
2. Add offline fixtures under `apps/desktop/src-tauri/tests/fixtures/` covering: success, partial/missing fields, malformed JSON, 401, 429 with `retry-after`, 5xx, and empty responses — whichever apply to the endpoints you declared.
3. Run `cargo test`. The manifest registry validates everything else.

Community providers (outside the built-in set) ship as [packs](README.md#community-provider-packs) instead — same schema, loaded at runtime.

## Ground rules

- **Secrets stay in the OS keychain.** Never place API keys, tokens, cookies, raw provider responses, or complete secrets in source, fixtures, SQLite, or logs. The secret-leak regression suite (`src-tauri/tests/secret_leak.rs`) byte-scans for violations — keep it green.
- **Unknown is not zero.** `Option`/`null` represents unavailable data; never fabricate quota limits or backfill missing history.
- **The webview stays network-confined.** Don't broaden Tauri capabilities or the CSP; all HTTP belongs in the Rust `networking` layer.
- **Preserve the failure invariant.** Failed refreshes keep the last good snapshot, mark it stale, and back off with bounds.

## Pull requests

- Keep PRs focused; one logical change per PR.
- Include screenshots or recordings for any UI-visible change (state variants, narrow windows, dark mode where relevant).
- CI must pass: frontend typecheck/tests, Rust fmt/clippy/tests, version consistency.
- For behavior changes, document the behavior in the PR description and relevant Rustdoc comments.

## Reporting vulnerabilities

Please don't open public issues for security reports — use [private security advisories](https://github.com/Aswellle/TokenOwl/security/advisories/new) (see [SECURITY.md](SECURITY.md)).
