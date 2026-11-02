# Changelog

All notable changes to TokenOwl are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Release notes are also published on the
[GitHub Releases page](https://github.com/Aswellle/TokenOwl/releases).

## [Unreleased]

## [1.5.0] — 2026-11-02

### Added

- The bubble draws activity rings from real quota windows: `lib/rings.ts`
  derives a consumed ratio per trustworthy window, assigns the shortest span
  to the outer band and the longest to the inner one, and keeps at most
  three semantically distinct dials — it never invents a third window.
  Bands use a bright same-hue gradient over a dimmer track, round caps, and a
  clear dark gap; zero and unknown values draw no arc at all, and full
  consumption switches to butt caps so two round caps cannot bulge the
  12 o'clock seam.
- Ring progress is tweened by GSAP: the first sight of a ring draws it from
  an empty dial with an outward-in stagger, later refreshes tween from the
  current value (a refresh never looks like a reset), reduced motion writes
  the target directly, and every tween lives in a `useGSAP` context so
  StrictMode's remount cycle cannot leave duplicates behind. The rings also
  compress and settle with the native window morph.
- The bottom dock collapses: one click returns its strip to the content
  area, and the same tab brings it back. The tab is centred on the dock's
  top edge and rides that edge through the animation — expanded it sits on
  the dock border, collapsed it rests on the window's bottom edge — with a
  rotating chevron instead of a swapped icon. The collapsed row leaves the
  tab order (`inert`). The preference persists through
  `dock_state_get`/`dock_state_set` in the settings table; a missing or
  malformed value means expanded, so a corrupted row can never hide the
  controls, and it loads before the transition is armed so a restart does
  not flash the dock open first.
- The dashboard window is sized to and centred inside the monitor's work
  area (screen minus taskbar / menu bar / Dock) before it is shown, so the
  bottom dock is visible at startup on both Windows and macOS instead of
  sitting under the taskbar. The window exists before the compositor
  reports a monitor, so a first miss hides the window and retries every
  100 ms for up to 2 s; the expand-from-bubble path centres within the same
  work area.
- Expired or revoked API keys recover in place: `accounts_update_credential`
  overwrites the keyring entry under the account's existing credential
  reference (account, provider row and history untouched) and validates the
  new key with an immediate refresh. Erroring account rows expose an
  update-key action with a masked inline dialog.
- An explicit proxy setting for corporate gateways: the URL (which may embed
  proxy credentials) lives in the OS credential store, never in SQLite, and
  is threaded through all three adapters at startup. Redirect denial and the
  no-implicit-proxy rule are unchanged; the setting takes effect on the next
  start, which the UI states.
- LAN account creation can remember a target: an opt-in checkbox in the
  consent dialog stores the exact base URL in a revocable trusted-target
  list, managed per row in settings. Only classifiable LAN targets can be
  trusted — public URLs need no consent and unclassifiable ones fail
  closed — and the one-time token path is unchanged for everything else.
- Command errors carry a stable `code` across IPC (`AuthExpired`,
  `RateLimited`, and so on) alongside the human message, so the UI can
  branch by category; an expired key now points at the row's inline key
  update instead of a generic retry loop.
- Quota-less providers (L1/L2) say so: the account view model carries the
  manifest's capability level, error-free empty rows read "this provider
  publishes no key-level quota", the picker marks those entries, and
  selecting one explains the empty dashboard up front.
- The notification settings panel groups its controls into named sections
  (system notifications, conditions, timing, quiet hours) with a rotating
  chevron and a live state summary in the collapsed header; the 5–1440
  minute reset-window constraint is visible copy instead of a hidden input
  attribute.
- The top-left brand slot uses a new geometric owl mark — one continuous
  silhouette with mask-cut eyes and beak, so it reads on the titlebar tile,
  the light dashboard surface, and the bubble's dark jewel. The application
  icon and tray icon are unchanged.

### Changed

- Adapter dispatch reads the persisted `api_format` (`anthropic`,
  `google-ai`, or none for OpenAI-compatible) instead of string-matching the
  provider-id prefix; legacy rows fall back to the prefix, so no migration
  is needed and a fourth wire protocol no longer means touching two places
  that can drift.
- Notification relative-time labels re-render through a module-scoped clock
  (`useSyncExternalStore`): one interval for any number of subscribers, and
  a tick no longer re-renders the whole notification list. The setup form
  validates the same state it submits instead of reading controlled inputs
  back out of the DOM.

### Fixed

- Credentials persist across restarts: `keyring` 3 compiles no backend by
  default, so the previous build silently used the in-memory mock — account
  creation "succeeded" and the first refresh after a restart could not
  resolve the secret. Each platform now enables its native store
  (`windows-native`, `apple-native`, `linux-native`).
- Account deletion removes its provider row once no account references it,
  so a deleted account cannot leave a provider entry that no refresh can
  ever reach; snapshots, refresh state and history still cascade.
- Manual, first-load and delete-triggered refreshes emit `quota:updated`, so
  the bubble stops rendering accounts that no longer exist, and the
  window-region morph honours the OS reduced-motion switch.
- A quota window with no reported start/end timestamps no longer sorts as a
  0-hour span and steals the outer band from a window with a real span: it
  falls back to the nominal span its tag already declares (7d → 168h), which
  is shown to the user as the window's identity.
- Provider quota surfaces corrected against official documentation:
  OpenRouter `/key` (previously a double-joined 404), SiliconFlow
  `/user/info`, Kimi's new balance schema, and OpenAI/Qwen moved to their
  honest capability levels after removing fabricated endpoints. DeepSeek's
  balance envelope surfaces granted and topped-up splits; Anthropic's
  rate-limit header prefixes carry their units and never invent a span.
- Quota cards adapt to each kind: window chips carry type and span
  (滚动窗口 · 5h), token counts use the 万/亿 scale, percentages read as a
  share, and windowless balances never grow a fake reset time or ratio bar.
  Failed history, notification and matrix reads report themselves with a
  retry instead of skeletons or false empty states; the detail page groups
  windowed dials apart from balances and meters.
- The critical-only notification help text follows the configured low-quota
  threshold instead of quoting a frozen 10%; the titlebar close light
  performs the collapse-to-bubble handoff its label promises.

### Security

- The proxy URL may embed proxy credentials, so it is stored in the OS
  credential store under its own reference rather than in the SQLite file;
  error messages stay constant so the URL never reaches a log.
- The LAN trusted-target list matches the exact base URL, is stored in the
  settings table, and is revocable per row; adding a target re-runs the
  backend LAN classifier and rejects anything that is not a LAN target.
- API-key inputs are masked password fields in both the setup form and the
  in-place update dialog, and the secret never enters React state.

### Compatibility

- No SQLite schema migration; v1.4.x local data reads as-is.
- New IPC commands — `accounts_update_credential`, `proxy_get`/`proxy_set`,
  `lan_trust_list`/`lan_trust_add`/`lan_trust_remove`, and
  `dock_state_get`/`dock_state_set` — are additive.
- The command error payload changed from a bare string to
  `{ code, message }`; the only consumer is the bundled frontend.
- `OpenAiCompatibleConfig` persists an `api_format` field (`None` for
  OpenAI-compatible); rows written by earlier versions fall back to their
  provider-id prefix.
- New frontend dependencies: `gsap` and `@gsap/react`, with the license
  notice included in the repository.

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
