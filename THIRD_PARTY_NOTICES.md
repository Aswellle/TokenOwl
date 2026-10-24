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
| `recharts` | MIT | Trend charts |
| `@tauri-apps/api` | MIT OR Apache-2.0 | IPC bindings |
| `@lobehub/icons` | MIT | Provider brand icons |

## Assets

The application icon is original artwork created for this project.

## Reporting

If you believe a dependency's license is mis-attributed here, please open an
issue so it can be corrected before the next release.
