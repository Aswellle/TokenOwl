# Changelog

All notable changes to TokenOwl are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Release notes are also published on the
[GitHub Releases page](https://github.com/Aswellle/TokenOwl/releases).

## [Unreleased]

## [1.4.0] — 2026-10-30

### Added

- Custom provider targets are configured by wire format: the setup form
  groups every built-in provider under its request format (OpenAI-compatible,
  Anthropic, Gemini) and lets a custom target pick one of the three, with the
  default host and probe endpoints following the choice. Native Anthropic and
  Gemini targets are probed by their adapters (key check plus rate-limit
  headers) instead of pretending at quota endpoints.
- Platform variants of one brand collapse into a single picker capsule with
  an ellipsis expander: Zhipu ships 国内版 and 国际版, Kimi ships 国内版 and
  国际版, and the Anthropic/Gemini OpenAI-compatible relays live behind the
  same mechanism. Four manifests are new (`zhipu-international`,
  `kimi-international` plus variant tags on the relays); every built-in
  provider now renders its official brand icon, and the remaining two
  gateways (one-api, litellm) keep the documented fallback glyph.
- The floating bubble is a real overlay: its window leaves the taskbar and
  Alt-Tab (`skipTaskbar` plus `WS_EX_TOOLWINDOW`, re-applied whenever the
  shell rewrites window flags), keeps an exact 56×56 logical footprint
  without the OS minimum-width inflation, and drops its square DWM shadow.
- The bubble ↔ dashboard handoff is an OS window-region animation owned by
  Rust: a `SetWindowRgn` circle grows out of the bubble's docked spot on
  expand and pulls the panel — acrylic material and content together — back
  into it on collapse. A CSS clip-path could not clip the OS material layer,
  which is what left a blank rectangle behind the shrinking panel before.
- The provider detail view is reachable from the account row (the card face
  is a button) and from every provider, since the rail's overflow indicator
  is now a real control; a back control and Escape return to the overview.
- The updater check renders its result next to the check button, both
  outcomes ("up to date", failure) included, each with a dismiss control and
  a download button when a release is available.
- A large dynamic title beside the brand names the live page, and section
  headers stop repeating it; empty states across the dashboard carry copy
  that matches their situation and next step.

### Fixed

- Setup form interaction: inline validation replaces the webview's
  system-language bubbles with per-field focus movement; a failed connection
  keeps the typed API key; a successful one confirms next to the submit
  button and resets the form; the variant menu dismisses on Escape, outside
  presses, and focus loss and paints on an opaque surface; the ellipsis
  expander meets the 24px target size.
- The detail view shows the provider's measured capability level from the
  embedded matrix instead of a hardcoded placeholder, and the usage cards
  stay neutral ("—") while their query is in flight instead of reading as
  "no records".
- Notification preferences roll back the edit that failed to save; the
  filtered-empty state has its own copy; touch hover suppression applies
  regardless of transparency settings.
- The empty bubble is a solo owl sized to the circle (no transform hack)
  with a reduced-motion-aware idle breath; quota rings are two separated
  bands (r=21/14, 2-unit gap, no channel circle) that draw no arc at zero or
  unknown remaining.
- The updater result is announced once (a nested status region made screen
  readers double-report it), and the tray icon is set while left clicks keep
  opening the dashboard.
- Refresh correctness: the single-flight lease releases through scope even
  when the refresh future is cancelled; credential-create rollback routes
  deletions through the startup cleanup queue; unknown reset times stay
  unknown; `stale_after` doubles as the tested refresh-cadence input;
  account ids derive from nanoid instead of the wall clock.
- Manifest registration validates the full request contract (relative
  endpoint paths, header names, registration base URLs) at both load
  boundaries; the taskbar icon renders from a high-resolution bitmap.

### Security

- Upgraded `rustls` to 0.23.46 (four advisories: debug-secret redaction, Brotli
  certificate-decompression allocation, ServerHello downgrade, ECH rejection
  buffering), `rustls-webpki` to 0.103.16 (distrusted-intermediate acceptance),
  `webpki-root-certs` to 1.0.11 (emSign intermediates), `time` to 0.3.47
  (parsing stack exhaustion), and `tokio` to 1.54.1 (AsyncFd I/O-safety
  soundness). The minimum Rust version moves to 1.88 — the patched `time`
  chain requires it — and nested conditionals collapse to let-chains as the
  compiler now suggests.
- The dependency audit allows five advisories that are blocked upstream, each
  with a written justification in `.cargo/audit.toml`: quick-xml (via
  tauri → plist, which only handles the app's own build-time configuration)
  and zip (via tauri-plugin-updater, whose archive is minisign-verified before
  extraction). Remove each entry as soon as `cargo update` can reach the
  patched versions.

### Compatibility

- No SQLite schema migration; v1.3.x local data reads as-is.
- New IPC command `stabilize_bubble_window` (the bubble webview re-asserts
  its own window style); `accounts_create` gained an optional `apiFormat`
  field and the manifest view model optional `brand`/`variant` fields — all
  additive, no shapes removed. The two former handoff commands
  (`finish_bubble_handoff`, `finish_dashboard_handoff`) are gone; their
  caller was the bundled frontend only.

## [1.3.0] — 2026-10-24

### Security

- All provider traffic now flows through one shared HTTP client that denies
  redirects by default, ignores environment proxies, and enforces the
  streaming response-size cap in a single place; provider manifests asking to
  follow redirects are rejected.
- Plaintext `http` base URLs are accepted only for loopback; remote and LAN
  targets must use `https`, and base URLs with a query or fragment are
  rejected outright.
- LAN target classification now understands IPv6 unique-local and
  IPv4-mapped addresses and resolves DNS names once, so a public-looking name
  that points inside the network still triggers the consent prompt.
- LAN consent is validated by the backend: a one-time, target-bound token is
  required exactly when Rust classifies the target as local, and the
  classifier failing means creation is blocked instead of guessed public.
- API keys no longer appear in `Debug` output of the account input DTO, and
  raw provider response bodies are never formatted; the sensitive-key
  matcher's dead entries (`auth_token`, `api_token`) were fixed and unified
  across metadata sanitizing and telemetry.
- Release workflows expose signing secrets only to the steps that need them,
  drop persisted checkout credentials, and pin every action to a verified
  commit SHA; the bubble window holds no plugin permissions.
- The resolved secret buffer is zeroized when dropped (best-effort defense in
  depth; copies that leave the credential type still follow ordinary Rust
  lifetimes).

### Fixed

- `manifest_profiles` is registered with the invoke handler again — the
  packaged app had been showing an empty provider list while browser previews
  kept passing.
- The retry backoff policy has non-zero defaults (30 s base, 15 min cap,
  bounded jitter); a provider's `Retry-After` (seconds or HTTP-date) is
  honored and capped when scheduling the next attempt.
- Refreshes are per-account single-flight with bounded parallel waves, so the
  background loop, the UI, and the hotkey can no longer race one account.
- A failed keyring deletion during account removal is recorded and retried on
  the next startup instead of silently orphaning the secret; a failed database
  open right after writing a secret compensates by deleting it.
- Migration backups are staged and quick-checked before replacing the previous
  generation, so a failed `VACUUM` can no longer destroy the only restorable
  copy.
- Community manifest scans are bounded (file count, per-file size, report
  length, regular files only), and manifest fields (id, name, timeout,
  mapping paths) are validated at both load boundaries.

### Changed

- Confirmations render through a shared accessible alert dialog (labelled,
  modal, Escape cancels, focus starts on the non-destructive action and
  returns to the trigger); the removal confirm also focuses Cancel first.
- The dashboard's setup form, account row, and notice are individual modules,
  and account/updater state lives in dedicated hooks (`useAccounts`,
  `useUpdater`) — `App.tsx` halved from ~1400 to ~710 lines with no behavior
  change.
- The unused priority-queue scheduler was removed; the refresh path is the
  single scheduling implementation.
- SECURITY/CONTRIBUTING/AGENTS documents now match the actual repository
  layout, supported versions, and README languages.

### Added

- Scheduled dependency audit workflow (weekly `cargo audit` plus
  `pnpm audit --prod --audit-level high`).
- The dashboard and bubble-view test suites run again in the default test
  run (they had been excluded when their mocks drifted).

## [1.2.0] — 2026-10-18

### Added

- Global hotkeys: `Alt+Shift+T` toggles between the bubble and the dashboard,
  `Alt+Shift+R` refreshes every account; a hotkey another app already owns is
  skipped at startup instead of blocking it.
- LAN consent: the first account creation pointing at a private network or the
  local machine now asks for confirmation (docs/03 §12), with numeric private
  range detection in Rust.
- Per-window quota alerts: the 5h, daily, weekly, and monthly windows each
  alert on their own threshold with their own dedup key, so the notification
  names the window that is actually running out instead of one account-level
  number.
- Predictive exhaustion warning: burn rate from the last 24 hours, fired only
  when the projection runs out before the window resets; disabled when history
  is too thin to project.
- Recovery notices: an account that recovers from disconnection or auth
  failure announces it once.
- Banner summarization: several conditions on one account arrive as a single
  banner inheriting the worst severity, while the notification centre keeps
  every condition.
- Quiet hours (22:00–08:00 default) that critical alerts pierce, and a
  configurable exhaustion-projection horizon.
- Notification centre filters by kind and tags each entry.
- Render-failure fallback: an uncaught render error now shows a recoverable
  message with a reload action instead of leaving a blank window.
- `CHANGELOG.md` and `THIRD_PARTY_NOTICES.md`.
- Bilingual README (Simplified Chinese default, English at `README.en.md`).

### Changed

- The bubble's outer activity ring sits flush against the bubble edge (it
  previously left a one-unit status-coloured rim), and the dial uses a dark
  jewel background so the two data rings carry the colour.
- Banner and sound behaviour: OS banners group by account, and critical
  notifications carry a default sound.

### Fixed

- Switching between the bubble and the dashboard no longer passes through a
  frame where neither surface is visible; both directions now share one
  spatial path.

## [1.1.0] — 2026-10-17

### Added

- New owl application icon across the app, tray, installers, and README,
  rounded with an Apple-style squircle.
- 29 built-in provider manifests across three wire protocols
  (OpenAI-compatible, Anthropic, Google AI) with honest L1–L4 grading.
- Native Anthropic and Gemini adapters that map rate-limit headers to windowed
  quota snapshots.
- Community provider packs: validated YAML manifests loaded from
  `community-providers`.
- Per-provider usage view with 2h / 24h / 30d windows, burn rate with projected
  exhaustion, and a 26-week usage heatmap.
- Hover tooltips with per-day detail on chart points.
- Seamless bubble ↔ dashboard window transition (spatial handoff, no blank
  frame).
- OAuth session credentials with expiring sessions in the OS keychain.

### Changed

- Bubble redesigned around Apple Watch-style dual rings with edge snapping.
- Settings overhauled: protocol-grouped provider picker, collapsible capability
  matrix, and honest unsupported-protocol labelling.
- Single-scroll dashboard layout.

### Fixed

- The setup form rendered as a non-form element, so the connect button silently
  did nothing.
- Gateway profiles discarded the user-supplied base URL and persisted the
  manifest placeholder, breaking every gateway refresh.
- Native adapters read a placeholder construction config instead of the account
  config, sending Anthropic/Gemini refreshes to the wrong host.
- The models endpoint was dropped from account config during creation.
- Accounts without snapshots yet no longer crash the summary view.
- Double vertical scrollbar in the dashboard shell.

## [1.0.0] — 2026-10-10

First public release: local-first quota monitoring for eight providers, floating
bubble, notifications, history, SQLite persistence, OS keychain credentials, and
a signed updater.
