# TokenOwl 实现差距矩阵

更新时间：2026-10-05

| Requirement | Source Document | Expected | Implemented | Status | Deviation | Risk | Next Action |
|---|---|---|---|---|---|---|---|
| Tauri 2 + React + TypeScript + Rust | 总规范 / 02 | 可启动桌面工程 | 官方模板骨架已建立 | Partial | 业务层尚未完成 | P1 | 完成首次 Tauri build 与入口集成 |
| 可运行 Git 基线 | 07 / 用户总控 | commit、branch、可回滚 checkpoint | 本地 Git 已初始化，尚无 checkpoint commit | Partial | 远程无可读取引用 | P1 | 首次验证通过后建立 checkpoint |
| Bubble + Dashboard | 01 / 05 / 10 | 展开、收起、低干扰悬浮球 | 单默认窗口模板 | Missing | 还没有 Bubble、Tray、拖拽 | P1 | 实现 shell 和状态切换 |
| 位置记忆 | 01 / 05 | 拖拽后持久化、多屏迁移 | 未实现 | Missing | 无设置存储 | P1 | 先实现 local settings IPC，再接 SQLite |
| Domain quota model | 02 / 03 / 04 | 多单位、多窗口、可信 reset、unknown | 未实现 | Missing | UI 无领域模型 | P0 | 建立 Rust domain 与单元测试 |
| SQLite migrations | 02 / 04 / 06 | WAL、foreign key、升级备份 | 未实现 | Missing | 无数据库 | P0 | 建立 migration 001 与 repository |
| OS credential store | 02 / 06 | keyring/credential manager，只保存 ref | 未实现 | Missing | 无 credential service | P0 | 接入 keyring 并做 secret redaction 测试 |
| Provider protocol | 03 | descriptor、capability、adapter、registry | 未实现 | Missing | 无 adapter contract | P0 | Generic adapter + fixture harness |
| Generic OpenAI-compatible | 01 / 03 / 10 | base URL、API key、headers、usage/quota/balance 可选 | 未实现 | Missing | 无网络层 | P0 | 建立统一 HTTP client 与明确 unavailable |
| Stale fallback / error isolation | 总规范 / 02 / 06 | 旧快照继续显示，错误分层 | 未实现 | Missing | 无 scheduler/store | P0 | 建立 refresh state 与 error model |
| 首发 Provider | 03 / 09 / 10 | OpenAI、Anthropic、Gemini、Qwen、DeepSeek、Kimi、MiniMax、OpenRouter | 未实现 | Missing | 未验证官方 API 能力 | P1 | 每个 provider 独立 fixture 和 capability |
| UI states | 05 / 06 / 07 | loading、empty、partial、stale、error、offline、keyboard | 未实现 | Missing | 模板 UI 仍为 greeting demo | P1 | 替换 App 为真实空状态与 snapshot view |
| CI / release | 06 / 10 | Windows/macOS build、签名、checksum、updater | 未实现 | Missing | 无 workflow | P1 | 先完成 Windows build，再增加 matrix |
