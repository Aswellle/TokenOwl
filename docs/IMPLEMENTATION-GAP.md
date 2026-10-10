# TokenOwl 实现差距矩阵

更新时间：2026-10-10

| Requirement | Source Document | Expected | Implemented | Status | Deviation | Risk | Next Action |
|---|---|---|---|---|---|---|---|
| Tauri 2 + React + TypeScript + Rust | 总规范 / 02 | 可启动桌面工程 | 前后端已打通账户命令与事件 | Partial | 本机缺 MSVC C++ 工作负载，Rust 侧只能格式检查 | P0 | 在装有 C++ 工具链的环境跑 `cargo test`/`clippy` |
| 可运行 Git 基线 | 07 / 用户总控 | commit、branch、可回滚 checkpoint | 本地 Git 已初始化，尚无 checkpoint commit | Partial | 远程无可读取引用 | P1 | 首次验证通过后建立 checkpoint |
| Bubble + Dashboard | 01 / 05 / 10 | 展开、收起、低干扰悬浮球 | 悬浮球 + 仪表盘、托盘打开/退出已实现 | Partial | 拖拽与位置记忆未接线 | P1 | 接入 `save_bubble_position` 与窗口拖动 |
| 位置记忆 | 01 / 05 | 拖拽后持久化、多屏迁移 | 命令与 clamp 逻辑存在，前端未调用 | Missing | 位置不持久化 | P2 | 前端接入 IPC 并处理多屏 |
| Domain quota model | 02 / 03 / 04 | 多单位、多窗口、可信 reset、unknown | `provider.rs` 归一化模型与 fixture 解析 | Partial | 只覆盖有限字段名，无多窗口时长推导 | P1 | 扩展解析并补 fixture 用例 |
| SQLite migrations | 02 / 04 / 06 | WAL、foreign key、升级备份 | v1 schema、WAL、外键、`user_version` | Partial | 无升级备份与 integrity 检查 | P1 | 增加迁移前备份与完整性校验 |
| OS credential store | 02 / 06 | keyring/credential manager，只保存 ref | `credentials.rs` 只存 ref，secret 不入库 | Done | 无 | P1 | 增加 secret 泄露回归测试 |
| Provider protocol | 03 | descriptor、capability、adapter、registry | 能力位、统一错误、adapter、registry | Partial | 无 Manifest/第三方加载 | P1 | 保持 adapter 边界，后续接 Manifest |
| Generic OpenAI-compatible | 01 / 03 / 10 | base URL、API key、headers、usage/quota/balance 可选 | HTTP 适配器 + 单 endpoint 解析 | Partial | 单一 endpoint，未合并多端点 | P1 | 支持 quota/usage/balance 多端点合并 |
| Stale fallback / error isolation | 总规范 / 02 / 06 | 旧快照继续显示，错误分层 | 失败保留快照、记录错误码与退避时间 | Partial | `stale_after` 未参与调度 | P1 | 后台按 `stale_after` 决定是否刷新 |
| 首发 Provider | 03 / 09 / 10 | OpenAI、Anthropic、Gemini、Qwen、DeepSeek、Kimi、MiniMax、OpenRouter | 未实现 | Missing | 未验证官方 API 能力 | P1 | 每个 provider 独立 fixture 和 capability |
| UI states | 05 / 06 / 07 | loading、empty、partial、stale、error、offline、keyboard | 各状态已实现且为简体中文，含明暗主题与额度进度条 | Partial | 无自动化测试覆盖；原生窗口未截图验证 | P1 | 补前端状态机测试 |
| CI / release | 06 / 10 | Windows/macOS build、签名、checksum、updater | 检查 + 三平台签名构建 + `latest.json` 上传 | Partial | 未在真实 release 上演练 | P1 | 用测试密钥跑一次 draft release |
| 自动更新运行时 | 06 / 11 | 检查、用户确认、签名校验、重启 | 插件已注册，命令 + 事件 + UI 提示完成 | Partial | 端到端未验证（缺签名资产） | P1 | 结合 draft release 验证安装与重启 |
| 健康分级阈值 | 01 / 05 | Critical/attention 按剩余比例与重置时间分级 | 只有 healthy/stale/unavailable/error | Missing | 未定义 attention 阈值，未实现比例计算 | P1 | 先确认阈值再实现并补测试 |
| 历史趋势与通知 | 01 / 10 | 历史图表、通知 | 未实现 | Missing | 无数据聚合 | P2 | 在设计确认后再开始 |
