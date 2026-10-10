<p align="center">
  <img src="apps/desktop/public/tokenowl-icon.png" alt="TokenOwl" width="128" height="128" />
</p>

<h1 align="center">TokenOwl</h1>

<p align="center">
  <strong>Local-first AI quota monitor for Windows and macOS</strong>
</p>

<p align="center">
  <a href="https://github.com/Aswellle/TokenOwl/actions/workflows/ci-frontend.yml"><img src="https://img.shields.io/badge/frontend-CI-blue?style=flat-square&logo=github" alt="Frontend CI" /></a>
  <a href="https://github.com/Aswellle/TokenOwl/actions/workflows/ci-rust.yml"><img src="https://img.shields.io/badge/rust-CI-blue?style=flat-square&logo=github" alt="Rust CI" /></a>
  <a href="https://github.com/Aswellle/TokenOwl/releases"><img src="https://img.shields.io/badge/release-v1.0.0-blue?style=flat-square" alt="Release" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-blue.svg?style=flat-square" alt="License" /></a>
</p>

---

## Why TokenOwl

You pay for API access to multiple AI providers — but hitting a quota ceiling mid-conversation is a frustrating surprise. TokenOwl lives in your system tray and watches your usage in real time, so you always know what you have left.

**Key benefits:**

- **Zero cloud** — Your data never leaves your machine. No accounts, no tracking, no subscription.
- **Privacy by design** — API keys live in the OS keychain (Windows Credential Manager / macOS Keychain), never in databases or logs.
- **Unified view** — One dashboard for OpenAI, Anthropic, Gemini, and five other providers.
- **Featherweight** — ~5 MB installer. Rust backend with a webview frontend keeps memory usage minimal.

## Features

| Feature | Description |
| :--- | :--- |
| **Real-time monitoring** | Background scheduler refreshes quota with jitter and exponential backoff |
| **Multi-provider** | 8 built-in adapters: OpenAI, Anthropic, Gemini, OpenRouter, DeepSeek, Qwen, Kimi, MiniMax |
| **Multi-account** | Add unlimited accounts with independent refresh cycles |
| **Smart notifications** | Configurable low-quota alerts, critical thresholds, reset reminders |
| **Floating bubble** | Always-on-top status bubble with drag-to-reposition and position persistence |
| **Glassmorphism UI** | Transparent acrylic window with smooth morph transitions |
| **Usage analytics** | Burn rate, average usage, pressure gauge, reset countdown |
| **Auto-update** | Signed Tauri updater with GitHub Releases integration |

## Screenshot

```
┌─────────────────────────────────────────────┐
│  TokenOwl                          ─ □ ✕  │
├─────────────────────────────────────────────┤
│  Pressure  [██████░░░░] 68%   ○ Need attention│
│  Accounts  2 configured                      │
│  Healthy   1                                 │
│  Next reset 3d 14h                           │
├─────────────────────────────────────────────┤
│  Accounts (2)                         [+][↻] │
│  ┌───────────────────────────────────────┐  │
│  │ ● OpenAI 官方    ████████░░  82%      │  │
│  │   Reset in 3d 14h  ·  Fresh           │  │
│  ├───────────────────────────────────────┤  │
│  │ ● Anthropic       ████░░░░  41%      │  │
│  │   Reset in 6d 2h   ·  Fresh           │  │
│  └───────────────────────────────────────┘  │
│                       TokenOwl v1.0.0 · Local│
└─────────────────────────────────────────────┘
```

## Supported Providers

| Provider | Quota | Usage | Balance | Models |
| :--- | :--- | :--- | :--- | :--- |
| OpenAI | ✓ | ✓ | ✓ | ✓ |
| Anthropic | ✓ | ✓ | — | ✓ |
| Gemini | ✓ | ✓ | — | ✓ |
| OpenRouter | ✓ | ✓ | ✓ | ✓ |
| DeepSeek | ✓ | ✓ | ✓ | ✓ |
| Qwen | ✓ | ✓ | ✓ | ✓ |
| Kimi | ✓ | ✓ | ✓ | ✓ |
| MiniMax | ✓ | ✓ | ✓ | ✓ |

Custom OpenAI-compatible endpoints are also supported.

## Installation

### Windows

Download the latest installer from [Releases](https://github.com/Aswellle/TokenOwl/releases):

- `TokenOwl_x.x.x_x64-setup.nsis.exe` — Recommended NSIS installer
- `TokenOwl_x.x.x_x64_en-US.msi` — MSI for enterprise deployment

### macOS

- `TokenOwl_x.x.x_aarch64.dmg` — Apple Silicon (M1/M2/M3)
- `TokenOwl_x.x.x_x64.dmg` — Intel

## Quick Start

1. Launch TokenOwl — a small bubble appears in the bottom-right corner
2. Click the bubble to open the dashboard
3. Click **Add Account** and enter your API key and base URL
4. Quota data loads automatically
5. Drag the bubble to your preferred position — it persists across restarts

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                    Frontend (React)                   │
│  Dashboard · Bubble · Settings · Notifications       │
└────────────────────────┬────────────────────────────┘
                         │ Tauri IPC
┌────────────────────────▼────────────────────────────┐
│                    Backend (Rust)                     │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐           │
│  │Scheduler │  │ Providers│  │  Domain  │           │
│  │ BinaryHeap│  │ 8 builtin│  │  Reset   │           │
│  │ + jitter │  │ adapters │  │  Balance │           │
│  └────┬─────┘  └────┬─────┘  │  History │           │
│       │              │        └──────────┘           │
│  ┌────▼─────┐  ┌────▼─────┐  ┌──────────┐           │
│  │  SQLite  │  │   HTTP   │  │  Keyring │           │
│  │   WAL    │  │  Client  │  │  OS API  │           │
│  └──────────┘  └──────────┘  └──────────┘           │
└─────────────────────────────────────────────────────┘
```

**Tech stack:**

- **Desktop shell** — [Tauri 2](https://tauri.app) with acrylic/vibrancy window effects
- **Backend** — Rust with `tokio`, `reqwest`, `rusqlite`, `serde`
- **Frontend** — React 19, TypeScript strict, Vite 8, `motion/react`
- **State management** — Lightweight React hooks (no external store)
- **Testing** — Vitest + Testing Library (12 tests), `cargo test` (117+ tests)
- **CI/CD** — 4 parallel GitHub Actions workflows, signed releases

## Development

### Prerequisites

- [Node.js](https://nodejs.org/) 22+
- [Rust](https://www.rust-lang.org/tools/install) stable 1.85+
- [pnpm](https://pnpn.io/) 10.33.2+ (`corepack enable`)
- [Tauri CLI](https://v2.tauri.app/start/prerequisites/) v2

### Setup

```bash
git clone https://github.com/Aswellle/TokenOwl.git
cd TokenOwl
corepack enable
cd apps/desktop
pnpm install
```

### Commands

```bash
pnpm tauri dev          # Development build with hot reload
pnpm build              # Production build
pnpm typecheck          # TypeScript type checking
pnpm test               # Frontend unit tests
cargo test              # Rust unit + integration tests
cargo fmt --all         # Format Rust code
cargo clippy -- -D warnings  # Lint Rust code
```

## Security

- API keys stored exclusively in the OS keychain
- HTTPS-only network requests
- Response size limits prevent memory exhaustion
- Sensitive fields automatically redacted in logs
- Path traversal protection on endpoint construction
- Content Security Policy restricts resource loading

See [SECURITY.md](SECURITY.md) for the full security policy and vulnerability reporting process.

## Contributing

Contributions are welcome. Please read [CONTRIBUTING](CONTRIBUTING.md) before submitting a pull request.

## License

[MIT](LICENSE) — Copyright (c) 2026 TokenOwl contributors
