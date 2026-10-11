# TokenOwl 实现差距矩阵

更新时间：2026-10-11

| Requirement | Source Document | Expected | Implemented | Status | Deviation | Risk | Next Action |
|---|---|---|---|---|---|---|---|
| Tauri 2 + React + TypeScript + Rust | 总规范 / 02 | 可启动桌面工程 | 前后端已打通账户命令与事件 | Partial | 本机缺 MSVC C++ 工作负载，Rust 侧只能格式检查 | P0 | 在装有 C++ 工具链的环境跑 `cargo test`/`clippy` |
| 可运行 Git 基线 | 07 / 用户总控 | commit、branch、可回滚 checkpoint | 本地 Git 已初始化，尚无 checkpoint commit | Partial | 远程无可读取引用 | P1 | 首次验证通过后建立 checkpoint |
| Bubble + Dashboard | 01 / 05 / 10 | 展开、收起、低干扰悬浮球 | 悬浮球 + 仪表盘、托盘打开/退出已实现 | Done | 无 | P1 | 无 |
| 位置记忆 | 01 / 05 | 拖拽后持久化、多屏迁移 | SQLite settings 表持久化，重启恢复 | Done | 无 | P1 | 无 |
| Domain quota model | 02 / 03 / 04 | 多单位、多窗口、可信 reset、unknown | 归一化模型 + 16 reset 边界测试 | Done | 无 | P1 | 无 |
| SQLite migrations | 02 / 04 / 06 | WAL、foreign key、升级备份 | `storage/migrations/` 版本化迁移（编号 SQL 嵌入）+ 迁移前后 integrity 检查 + `VACUUM INTO` 备份 + 逐版本事务回滚 | Done | 备份仅保留最近一代；不自动还原，损坏库拒绝打开（safe mode） | P1 | 无 |
| OS credential store | 02 / 06 | keyring/credential manager，只保存 ref | `credentials.rs` 只存 ref，secret 不入库 | Done | 无 | P1 | 增加 secret 泄露回归测试 |
| Provider protocol | 03 | descriptor、capability、adapter、registry | ProviderAdapter trait + AdapterRegistry + Manifest 解析 | Done | 无 | P1 | 无 |
| Generic OpenAI-compatible | 01 / 03 / 10 | base URL、API key、headers、usage/quota/balance 可选 | HTTP 适配器 + 单 endpoint 解析 | Partial | 单一 endpoint，未合并多端点 | P1 | 支持 quota/usage/balance 多端点合并 |
| Stale fallback / error isolation | 总规范 / 02 / 06 | 旧快照继续显示，错误分层 | 失败保留快照、动态 stale_after 刷新间隔 | Done | 无 | P1 | 无 |
| 首发 Provider | 03 / 09 / 10 | OpenAI、Anthropic、Gemini、Qwen、DeepSeek、Kimi、MiniMax、OpenRouter | 8 个 built-in manifests + 7 fixture 测试 | Partial | 未验证官方 API 能力 | P1 | 每个 provider 独立 fixture 和 capability |
| UI states | 05 / 06 / 07 | loading、empty、partial、stale、error、offline、keyboard | 各状态已实现，12 前端测试通过 | Done | 无 | P1 | 无 |
| CI / release | 06 / 10 | Windows/macOS build、签名、checksum、updater | Windows MSI+NSIS 已验证；CI 路径正确 | Partial | macOS 未验证；未在真实 release 上演练 | P1 | 用测试密钥跑一次 draft release |
| 自动更新运行时 | 06 / 11 | 检查、用户确认、签名校验、重启 | 插件已注册，命令 + 事件 + UI 提示完成 | Partial | 端到端未验证（缺签名资产） | P1 | 结合 draft release 验证安装与重启 |
| 健康分级阈值 | 01 / 05 | Critical/attention 按剩余比例与重置时间分级 | `domain/health.rs` 按 PRD §5.1 阈值（<10% critical、<25% attention，严格边界）分级，账户状态透传到 bubble/仪表盘；条形色调阈值同步统一 | Done | 仅剩余比例参与分级（reset 邻近度留给通知阶段）；error 优先于数值分级 | P1 | 无 |
| 历史趋势与通知 | 01 / 10 | 历史图表、通知 | 未实现 | Missing | 无数据聚合 | P2 | 在设计确认后再开始 |
