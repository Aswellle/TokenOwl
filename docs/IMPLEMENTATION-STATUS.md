# TokenOwl 实施状态

更新时间：2026-10-10

本文只描述已经在仓库中存在的实现和已经执行过的验证。未在 Windows 之外运行过的行为一律标为"未验证"。

## Completed

- 阅读并核对 README 与全部正式技术文档；原始仓库没有源码、依赖、测试或 CI，基线记录见 [BASELINE.md](BASELINE.md)。
- Tauri 2 + React + TypeScript 工程骨架：产品名 `TokenOwl`、标识 `com.tokenowl.app`、托盘菜单、`main` 仪表盘窗口与 `bubble` 悬浮窗口配置。
- Monorepo 目录结构：`apps/desktop/`（前端 + Rust 后端）、`packages/`、`providers/`、`scripts/`。
- Rust 分层架构：`domain/`（provider、state、reset、balance、history）、`commands/`、`providers/`（builtin、manifest、registry）、`scheduler/`、`storage/`、`credentials/`、`networking/`、`telemetry/`、`notifications/`、`app/`。
- Provider Protocol：`ProviderAdapter` trait、`ProviderRegistry`、`ProviderDescriptor`、`ProviderCapabilities`、`AccountContext`、`FetchContext`、`AccountIdentity`、`UsageRange`、`UsagePoint`、`HealthStatus`。
- YAML Manifest 解析：`ProviderManifest` → `OpenAiCompatibleConfig` 转换，内置 8 个 provider manifests（openai、anthropic、gemini、openrouter、deepseek、qwen、kimi、minimax）。
- Built-in Providers：`BuiltinProviders` 模块，`builtin_registry()` 注册所有 8 个 provider adapters。
- Reset 计算引擎：`ResetCalculator` 支持 rolling、fixed、calendar、subscription、custom 窗口类型，provider-supplied reset_at 优先。
- Balance 显示：`Balance` 和 `BalanceBuckets` 类型，支持 paid、free_grant、bonus、subscription、promo、team_pool 多桶额度。
- History 趋势：`HistoryPoint` 和 `HistorySeries`，支持 burn rate 计算和平均使用率。
- Scheduler：`Scheduler` 基于 BinaryHeap 优先级队列，支持 jitter、backoff、per-provider 配置。
- 统一 HTTP 客户端：`networking::HttpClient` 提供 timeout、connect timeout、response size limit、sensitive header redaction。
- Telemetry：`telemetry` 模块提供结构化日志事件和 secret redaction。
- Notifications：`Notification`、`NotificationPreferences`、`NotificationFilter` 支持低额度警告和严重额度警报。
- Rust 领域原语：`src-tauri/src/provider.rs`（provider 描述符、能力位、统一 `ProviderError`、归一化 `QuotaSnapshot`、OpenAI-compatible 响应解析与 registry）和 `src-tauri/src/state.rs`（账户元数据、freshness 策略、位置、刷新状态、退避策略）。
- SQLite 持久化：`src-tauri/src/persistence.rs` 提供 WAL、外键、schema 版本与 `providers`/`accounts`/`snapshots`/`refresh_states` 表；只写入归一化数据，不写入原始响应体或凭证。
- OS 凭证库：`src-tauri/src/credentials.rs` 基于 `keyring` 封装，SQLite 只保存 `credential_ref`；`CredentialValue` 的 `Debug` 输出为 `REDACTED`，不实现序列化。
- Provider 网络适配器：`src-tauri/src/openai_adapter.rs` 只允许 http(s) base URL 加相对 endpoint，设置超时与响应大小上限，凭证在请求时解析且不写入日志。
- 刷新编排：`src-tauri/src/refresh.rs` 在失败时保留上一个快照、记录结构化错误码，并按 `BackoffPolicy` 写入 `next_attempt_at`。
- Tauri 命令与事件：`src-tauri/src/commands.rs` 暴露账户列表/创建/删除/刷新，创建后立即尝试取首次快照；`src-tauri/src/lib.rs` 注册托盘、后台刷新循环（600s）与 `quota:updated` 事件。
- 前端仪表盘：`src/App.tsx` 展示悬浮球 + 账户卡片，覆盖 loading/empty/unavailable/stale/error/offline 状态、键盘可达的原生控件、未知值显示为 "Unknown / Quota value unavailable"，并订阅 `quota:updated`。
- 自动更新：注册 `tauri-plugin-updater`/`tauri-plugin-process`，`updater_check`/`updater_install` 命令由用户确认后下载、校验签名、安装并重启；`app:update-available` 事件驱动 UI 提示。
- 发布基础：`package.json` 为版本唯一来源，CI 校验 package/Cargo 版本一致性与 tag 形式，tag 触发 Windows x86_64 与 macOS arm64/x86_64 签名构建并上传 `latest.json`；规则见 [11-发布与更新生命周期.md](11-发布与更新生命周期.md)。

## Verified on this machine (Windows 11, 2026-10-10)

- `pnpm typecheck`、`pnpm build`、`pnpm test`、`cargo fmt --check`、`cargo clippy --all-targets --all-features -- -D warnings`、`node` 配置 JSON 解析、`git diff --check` 均通过。
- `pnpm test` 当前输出 `tests 0`：前端没有测试文件，通过不代表有覆盖。
- Rust：`cargo test` **89 个测试全部通过**，`cargo clippy --all-targets -- -D warnings` 0 错误 0 警告。
- 应用启动冒烟：`tokenowl.exe` 启动后进程存活 ≥10s、无 stderr 输出、可正常关闭（窗口已创建）。
- `pnpm tauri dev` 已跑通（配合本机端口覆盖配置）：BeforeDevCommand 起 Vite（127.0.0.1:1800）→ `cargo run` → `Finished dev profile`，应用运行至超时被正常终止。
- 前端只依赖 `@tauri-apps/api`；已移除未使用的 `@tauri-apps/plugin-{opener,process,updater}` npm 包并刷新 `pnpm-lock.yaml`（此前也导致 tauri CLI 报 npm/crate 版本不匹配警告），`pnpm typecheck`/`pnpm build` 复验通过。
- Tauri 版本对齐：`tauri` crate 2.11.6（crates.io 最新 2.x）与 `@tauri-apps/api` 固定为 `~2.11`（解析到 2.11.1），消除 CLI 的 major/minor 不匹配警告。

## 本机构建环境（已验收）

只保留一个 Rust 工具链 `stable-x86_64-pc-windows-msvc`，链接器来自 Visual Studio Build Tools 14.44.35207，SDK 使用已安装的 Windows SDK 10.0.26100.0。

普通终端没有 VC 环境变量，因此构建/测试必须走以下任一入口：

- 机器本地脚本（已创建）：`C:\Users\welle\AppData\Local\tools\tokenowl-dev.cmd`，它会 `call vcvars64.bat`、设置 `CARGO_TARGET_DIR` 并进入仓库根目录。
- 或手动导出等价环境（路径随安装版本变化）：
  `PATH += <BuildTools>\VC\Tools\MSVC\<ver>\bin\Hostx64\x64`
  `LIB = <ver>\lib\x64;<SDK>\Lib\<sdkver>\um\x64;<SDK>\Lib\<sdkver>\ucrt\x64`
  `INCLUDE = <ver>\include;<SDK>\Include\<sdkver>\ucrt;<SDK>\Include\<sdkver>\um;<SDK>\Include\<sdkver>\shared`

MSVC 产物写入 `src-tauri/target-msvc`（由 `CARGO_TARGET_DIR` 指定），与 Cargo 默认的 `target/` 分离；已删除无法运行 tauri 产物的 GNU 工具链及其 MinGW 依赖（GNU 编译可通过，但 tauri 二进制在本机加载即失败 `0xC0000139`）。

### Windows 端口保留区间与 `tauri dev`

本机 `winnat`（Hyper-V/WSL NAT）保留了 TCP 区间，默认 dev 端口 1420 落在保留区间内，`pnpm tauri dev` 会以 `listen EACCES ::1:1420` 失败。这不是仓库配置问题。两种处理：

- 免管理员：换端口本地覆盖，`C:\Users\welle\AppData\Local\tools\tokenowl-dev-config.json` 把 `devUrl`/`beforeDevCommand` 指到空闲端口（例如 1800），然后
  `pnpm tauri dev --config C:\Users\welle\AppData\Local\tools\tokenowl-dev-config.json`。
- 管理员：`net stop winnat && net start winnat` 释放后再试，或先为该项目保留端口
  `netsh int ipv4 add excludedportrange protocol=tcp startport=1420 numberofports=1`。

用 `netsh interface ipv4 show excludedportrange protocol=tcp` 查看当前占用区间。

## 界面与排版（本轮改造）

- 默认语言改为简体中文：`index.html` 使用 `lang="zh-Hans"`，全部用户可见文案集中在 `src/copy.ts`（状态、单位、窗口类型、时间、错误与更新提示），组件不再内联英文串。
- 字体栈（不下载网络字体，CSP 保持严格）：拉丁与数字用 `system-ui` / `Segoe UI Variable Text` / `Segoe UI`，中文回退 `Microsoft YaHei UI` / `Microsoft YaHei` / `PingFang SC` / `Noto Sans CJK SC`；接口地址与额度接口等机器值使用等宽栈（`Cascadia Mono` / `Consolas` / `Sarasa Mono SC`）。
- 中文排版细节：正文行高 `1.75`、标题 `1.4`；`line-break: strict` 避免标点前断行，`overflow-wrap` 处理长 URL；语义字号 12/13/15/17/20/26，辅助文字不小于 12px；所有数值使用 `tabular-nums`，倒计时更新不产生跳动。
- 视觉系统：语义 token + 明暗双主题（`prefers-color-scheme`），单一低饱和主色；取消"卡片套卡片"，改用分隔线与留白分组；新增额度进度条（剩余比例，阈值 ≤15% 危险、≤40% 警示），并同时给出百分比文字与状态图标，避免只靠颜色表达状态。
- 交互与可达性：图标改为内联 SVG；图标按钮具备可访问名；移除账户改为内联确认（`role="alertdialog"`、焦点移入、Escape 取消）；更新提示改为可操作横幅（不再使用 `window.confirm`）；命中尺寸不小于 32px。
- 渲染验证（Chromium，1280 与 375 宽、明暗两色）：无控制台错误；375 宽下无横向溢出（`scrollWidth == clientWidth`）；Tab 顺序与可访问名正确、焦点环可见（2px）；对比度实测辅助文字 6.95:1、状态 chip 10.69:1（均高于 AA 的 4.5:1）。

## Blocked / Not verified

- macOS app bundle、Keychain、menu bar 与 Apple Silicon 行为未验证。
- 真实 provider 账号未接入；所有解析行为只由 `provider.rs` 中的离线 fixture 覆盖。
- updater 的端到端流程（签名资产、`latest.json`、安装与重启）未在真实 release 上执行；仓库内只有公钥和 CI 步骤。

## Not Started

- 前端 Zustand 状态管理迁移。
- SQLite migrations 目录结构（版本化 + 备份 + 完整性检查）。
- 虚拟列表（大量 Provider 性能优化）。
- 多显示器支持（边缘吸附 + 位置记忆）。
- Apple 签名/公证与稳定版发布流程演练。
- 前端自动化测试覆盖。

## Known Limitations

- `FreshnessPolicy.stale_after` 目前只作为策略字段保留，UI 的 Fresh/Stale/Expired 由 `fresh_for` 与 `hard_expire_after` 决定；后台刷新固定 600s，未按 `stale_after` 触发。
- 悬浮球位置命令（`get/save_bubble_position`）已存在，但前端拖拽尚未调用它们，位置不持久化。
- OpenAI-compatible 解析只识别 `used/limit/remaining`、`reset_at`、`unit`、`currency` 等有限字段名；无法识别的响应会导致 `InvalidResponse`，不会伪造数值。
- 一个账户一个 endpoint（默认 `/usage`），暂不支持 balance/usage 多端点合并。
- Bubble 与主窗口都由 Vite 单页应用渲染，未区分独立入口。

## Known Bugs

- 无已知会导致数据错误或数据泄露的缺陷；未修复项以"Known Limitations"记录。

## Security

- 真实 secret 只进入 OS 凭证库；SQLite、fixture、日志和 DTO 中都不含明文凭证。
- CSP 保持 `default-src 'self'`，未开放额外网络 capability；provider 请求只由 Rust 侧发起。
- 网络请求有超时与响应体积上限，并拒绝绝对 URL、`//` 前缀与含凭据的 base URL。
- telemetry 模块提供 sensitive header redaction；所有日志输出经过脱敏处理。

## Testing & Build

- 前端：`pnpm typecheck`、`pnpm build`；无自动化 UI 测试。
- Rust：`cargo fmt --check` 通过；`cargo test` 89 个测试通过；`cargo clippy --all-targets -- -D warnings` 0 错误 0 警告。
- CI：`.github/workflows/ci-release.yml` 运行 typecheck、test、build、fmt、check、clippy、cargo test 与版本一致性校验。

## Next Recommended Phase

1. 前端 Zustand 状态管理迁移。
2. SQLite migrations 目录结构。
3. 在装有 Visual Studio C++ 工作负载的机器或 CI 上跑通 `cargo test`/`cargo clippy`。
4. 接入第一个真实 provider 的官方限额端点。
5. 补前端测试覆盖账户状态机。
