# TokenOwl 实施状态

更新时间：2026-10-05

## Completed

- 阅读并核对 README 与全部正式技术文档。
- 确认原始仓库没有源码、Git 提交、依赖、测试或 CI。
- 记录原始环境与命令结果到 [BASELINE.md](BASELINE.md)。
- 使用官方 Tauri 2 React TypeScript 模板建立工程骨架。
- 设置产品名 `TokenOwl`、应用标识 `com.tokenowl.app`、Rust library 入口与初始桌面窗口配置。
- 安装前端依赖并生成 `package-lock.json`。

## In Progress

- Phase 1 Desktop Foundation：Bubble/Dashboard shell、窗口交互、位置持久化接口、Tray。
- Phase 2 Domain & Persistence：Rust 领域模型、SQLite migration、OS credential reference、scheduler 状态。
- Phase 3 Provider Framework：能力声明、统一错误、Generic OpenAI-compatible adapter 与离线 fixture。

## Not Started

- 首发厂商 Provider 的真实官方接口接入。
- 历史分析与通知。
- Windows/macOS 双平台构建矩阵、签名、更新器与发布资产。
- Manifest/WASM 第三方 Provider 安全沙箱。

## Known Limitations

- 当前只能在 Windows 环境验证 Windows 行为；macOS app bundle、Keychain、menu bar 和 Apple Silicon 构建尚未本地验证。
- 原始远程仓库没有可读取分支引用，因此当前工程是首个本地实现基线。
- Tauri CLI 使用项目本地 `@tauri-apps/cli`，尚未安装全局 CLI。
- 尚未接入真实账号；所有开发数据必须来自离线 fixture 或明确的空状态。

## Known Bugs

- 初始化模板迁移期间曾出现一次 Tauri JSON 数组闭合错误，已修复；后续构建必须覆盖该配置。

## Tests / Build

- 原始基线命令因缺少工程文件失败，详见 BASELINE.md。
- 模板迁移后的首次完整验证待执行。

## Cross Platform

- Windows：工具链已确认，应用运行与安装构建待验证。
- macOS：未验证。

## Security

- 当前没有 secret、真实 provider response 或账户数据。
- 正式实现必须使用 OS credential store，SQLite 只保存 `credential_ref`。
- Tauri CSP 已设置为默认 self 资源策略，网络 capability 尚未开放。

## UX

- 正式实现采用低干扰 Bubble + Dashboard 两态、明确 stale/unsupported/error 状态、键盘焦点和 reduced motion。
- 当前仍是工程初始化阶段，尚无可验收的最终视觉交互。

## Technical Debt

- 需要补齐自动化 lint/test 脚本、Rust clippy 配置、migration upgrade test 和 CI workflow。
- 已使用 TokenOwl 品牌资源替换 Tauri 模板图标，并生成桌面、iOS 与 Android 图标变体。

## Next Recommended Phase

完成 Phase 1/2 的最小可运行闭环：应用可启动，用户可看到明确的空状态，领域模型可计算真实字段，UI 不假造 provider 数据，Rust 测试和前端构建通过。
