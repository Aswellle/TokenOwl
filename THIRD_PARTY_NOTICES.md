# Third-Party Notices

TokenOwl is distributed under the [MIT License](LICENSE). It builds on the
following open-source projects. This file lists the direct dependencies that
ship in the desktop application, with the license each declares.

Transitive dependencies are not enumerated here; their licenses are compatible
with MIT and are recorded in `apps/desktop/src-tauri/Cargo.lock` (Rust) and
`pnpm-lock.yaml` (JavaScript), which pin every version that ships.

## Rust crates

| Crate | License | Use |
| :--- | :--- | :--- |
| `tauri` | MIT OR Apache-2.0 | Desktop shell, windows, IPC |
| `tauri-plugin-notification` | MIT OR Apache-2.0 | OS notifications |
| `tauri-plugin-opener` | MIT OR Apache-2.0 | Opening the project page |
| `tauri-plugin-process` | MIT OR Apache-2.0 | Restart after update |
| `tauri-plugin-updater` | MIT OR Apache-2.0 | Signed auto-update |
| `reqwest` | MIT OR Apache-2.0 | HTTP client |
| `tokio` | MIT | Async runtime |
| `rusqlite` | MIT | SQLite bindings (bundled SQLite is public domain) |
| `serde` / `serde_json` | MIT OR Apache-2.0 | Serialization |
| `serde_yaml` | MIT OR Apache-2.0 | Provider manifest parsing |
| `keyring` | MIT OR Apache-2.0 | OS credential store |
| `chrono` | MIT OR Apache-2.0 | Date/time handling |
| `url` | MIT OR Apache-2.0 | URL parsing and validation |
| `thiserror` | MIT OR Apache-2.0 | Error types |
| `anyhow` | MIT OR Apache-2.0 | Error context |
| `tracing` | MIT | Structured logging |
| `nanoid` | MIT | Identifier generation |
| `zeroize` | MIT OR Apache-2.0 | Secret buffer zeroing on drop |
| `async-trait` | MIT OR Apache-2.0 | Async trait support |

## JavaScript packages

| Package | License | Use |
| :--- | :--- | :--- |
| `react` / `react-dom` | MIT | UI rendering |
| `motion` | MIT | Animation |
| `gsap` | GSAP Standard "No Charge" License — https://gsap.com/community/standard-license/ | Quota-ring progress and bubble morph timelines inside the WebView |
| `@gsap/react` | GSAP Standard "No Charge" License — https://gsap.com/community/standard-license/ | `useGSAP()` React lifecycle integration |
| `recharts` | MIT | Trend charts |
| `@tauri-apps/api` | MIT OR Apache-2.0 | IPC bindings |
| `@lobehub/icons` | MIT | Provider brand icons |

GSAP is **not** MIT-licensed: it is distributed under the GSAP Standard
"No Charge" License, which permits use in websites, web apps and digital
interfaces and restricts building competing no-code animation tools with it.
TokenOwl is a desktop application whose UI renders in a WebView and is not an
animation authoring tool; the distribution scenario should still be reviewed
against that license by the project maintainer before each release.

## Assets

The application icon is original artwork created for this project.

## Reporting

If you believe a dependency's license is mis-attributed here, please open an
issue so it can be corrected before the next release.
