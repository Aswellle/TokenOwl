# Changelog

All notable changes to TokenOwl are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Release notes are also published on the
[GitHub Releases page](https://github.com/Aswellle/TokenOwl/releases).

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
