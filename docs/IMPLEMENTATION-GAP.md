# TokenOwl 实现差距矩阵

更新时间：2026-10-11

| Requirement | Source Document | Expected | Implemented | Status | Deviation | Risk | Next Action |
|---|---|---|---|---|---|---|---|
| Tauri 2 + React + TypeScript + Rust | 总规范 / 02 | 可启动桌面工程 | 前后端命令/事件打通；本机 MSVC 工具链可用，cargo test 131/ clippy 零警告，MSI+NSIS 已产出 | Done | 无 | P0 | 无 |
| 可运行 Git 基线 | 07 / 用户总控 | commit、branch、可回滚 checkpoint | main 完整提交历史，双阶段成果已推送 origin（d2778a5、eaf8329） | Done | 无 | P1 | 无 |
| Bubble + Dashboard | 01 / 05 / 10 | 展开、收起、低干扰悬浮球 | 悬浮球 + 仪表盘、托盘打开/退出已实现 | Done | 无 | P1 | 无 |
| 位置记忆 | 01 / 05 | 拖拽后持久化、多屏迁移 | SQLite settings 表持久化，重启恢复 | Done | 无 | P1 | 无 |
| Domain quota model | 02 / 03 / 04 | 多单位、多窗口、可信 reset、unknown | 归一化模型 + 16 reset 边界测试 | Done | 无 | P1 | 无 |
| SQLite migrations | 02 / 04 / 06 | WAL、foreign key、升级备份 | `storage/migrations/` 版本化迁移（编号 SQL 嵌入）+ 迁移前后 integrity 检查 + `VACUUM INTO` 备份 + 逐版本事务回滚 | Done | 备份仅保留最近一代；不自动还原，损坏库拒绝打开（safe mode） | P1 | 无 |
| OS credential store | 02 / 06 | keyring/credential manager，只保存 ref | `credentials.rs` 只存 ref，secret 不入库；secret 泄露回归套件就位（SQLite 字节扫描+正反对照、DTO/Debug 序列化、metadata 清洗、keyring BadEncoding 字节、telemetry 掩码，共 10 项） | Done | 无 | P1 | 无 |
| Provider protocol | 03 | descriptor、capability、adapter、registry | ProviderAdapter trait + AdapterRegistry + Manifest 解析 | Done | 无 | P1 | 无 |
| Generic OpenAI-compatible | 01 / 03 / 10 | base URL、API key、headers、usage/quota/balance 可选 | quota/usage/balance 多端点合并：按路径去重、部分失败生成错误指标快照、全部失败走 stale fallback；表单支持三端点配置 | Done | JSONPath/JMESPath 映射、LAN 授权提示仍属 manifest 增强（docs/03 §12） | P1 | 无 |
| Stale fallback / error isolation | 总规范 / 02 / 06 | 旧快照继续显示，错误分层 | 失败保留快照、动态 stale_after 刷新间隔 | Done | 无 | P1 | 无 |
| 首发 Provider | 03 / 09 / 10 | OpenAI、Anthropic、Gemini、Qwen、DeepSeek、Kimi、MiniMax、OpenRouter | 8 个 built-in manifests + 7 fixture 测试 | Partial | 未验证官方 API 能力 | P1 | 每个 provider 独立 fixture 和 capability |
| UI states | 05 / 06 / 07 | loading、empty、partial、stale、error、offline、keyboard | 各状态已实现，12 前端测试通过 | Done | 无 | P1 | 无 |
| CI / release | 06 / 10 | Windows/macOS build、签名、checksum、updater | Windows MSI+NSIS 已验证；draft release 演练完成：tauri-action 根目录 pnpm bug 已修复（projectPath），v1.0.1 草稿五件签名资产齐全，Windows+macOS(aarch64) 构建绿 | Partial | macOS 构建已 CI 验证但运行时仍未验证；草稿待手动 publish | P1 | 验证草稿资产后 publish，观察正式端点 |
| 自动更新运行时 | 06 / 11 | 检查、用户确认、签名校验、重启 | 插件已注册，命令 + 事件 + UI 提示完成；本地端到端演练通过（CI 签名资产 + 本地静态端点：检查→下载→minisign 验签→NSIS 静默安装→重启进 1.0.1，注册表/文件版本确认） | Partial | GitHub 正式端点的线上链路待草稿 publish 后验证 | P1 | publish v1.0.1 后跑一次线上更新 |
| 健康分级阈值 | 01 / 05 | Critical/attention 按剩余比例与重置时间分级 | `domain/health.rs` 按 PRD §5.1 阈值（<10% critical、<25% attention，严格边界）分级，账户状态透传到 bubble/仪表盘；条形色调阈值同步统一 | Done | 仅剩余比例参与分级（reset 邻近度留给通知阶段）；error 优先于数值分级 | P1 | 无 |
| 通知触发链路 | 01 §12 / 02 §7 / 04 §11 | 低额度/余额下限/reset 临近/认证过期/长时间失联 → 系统通知；静默模式、只提醒 Critical；SQLite 通知历史 | 纯函数触发判定 + `notifications` 表（迁移 v2，24h 去重冷却 + 保留上限）+ `tauri-plugin-notification` Rust 端派发（`request_permission` 授权守卫）+ `notification:enqueue` 事件 + 通知中心/偏好设置 UI | Done | OS 通知仅由 Rust 刷新链路触发，webview 零通知权限（用户确认的权限边界）；"OAuth/API key 即将过期"仅在明确可判时提醒——当前仅覆盖已被拒绝的情形；Windows 横幅派发链路已实机走通（演练钩子落库 + OS sink 调用），屏上渲染待人眼确认（依赖专注助手状态）；macOS 首次授权由系统在首次派发时自动弹出（插件 2.4 桌面端权限 API 为恒 Granted 桩，守卫为前向兼容） | P1 | 人眼确认横幅渲染（TOKENOWL_TEST_BANNER=1） |
| 历史趋势与图表 | 01 §13 / 10 | 最近 7/30 天 usage、窗口变化 | 迁移 v3 `history_points` 采集 + 按日聚合（token 正增量/计费/本地时区日桶）+ `history_summary` 命令 + GitHub 风格热力图（26 周 Tokens/计费、近 30 天统计卡、方向键漫游） | Done | "每个 quota window 的变化"未做（需按指标序列查询）；用量从接入后开始记录，安装前的日期为无数据而非零 | P1 | 无 |
| 桌面视图导航与气泡双环 | 用户需求 / 05 | 供应商图标导轨 + 分视图导航 + 2h/24h/月用量切换 + 底部功能区 + 气泡双环与贴边吸附 | 应用外壳重构：导轨（最多 5 图标 + 全部面板）+ 方向感知视图切换 + 底部 Dock（概览/历史/通知/设置/GitHub 站点图标外链）+ recharts 趋势面积图（全量/按账户）+ lobehub 品牌图标前缀映射+ `usage_windows` 命令 + Apple-Watch 风格双环（5h/7d 窗口自动映射）+ 48px 边沿吸附持久化 | Done | 2h/24h 窗口统计依赖本机历史（记录起始前显示"暂无记录"）；generic 兼容账户显示中性图标；气泡位置持久化与窗口创建已实机验证；双环渲染与拖拽贴边吸附待人工交互验证 | P1 | 人工验证双环/拖拽贴边/重启恢复（1 分钟清单） |
