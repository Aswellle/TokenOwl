# TokenOwl 多维度代码审计报告（2026-11-01）

审查技能：`code-review`、`security-audit`（guidance 模式，聚焦安全面）、`frontend-product-ux-audit`、`better-*` 系列（上一批次已加载并沿用其标准）。
审查范围：全仓库。证据等级区分 **Observed**（实机/运行时验证）、**Code-confirmed**（源码确认）、**Likely**（推断，标注验证方式）。安全审计为 guidance 模式的聚焦审查，非完整六阶段渗透审计。

---

## 1. 总评

架构骨架健康：Rust 独占持久化/凭据/网络，IPC 只传语义命令与 DTO；传输层集中唯一（拒绝重定向、显式禁代理、流式大小上限、敏感头脱色）；凭据走 OS keyring + `Zeroizing`；测试覆盖（vitest 164 / e2e 110 / cargo 322）真实且分层。**主要短板不在已建成的部分，而在"长跑场景"与"失败长尾"**：密钥失效后的恢复路径缺失、无额度能力供应商的预期管理、结构化错误缺失导致的可操作性上限，以及企业代理用户被完全锁死。

## 2. 供应商配置的 UX 障碍与死路（用户目标重点）

### P1-1 无密钥更新/重授权路径 —— 密钥失效即死路
- **Evidence (Code-confirmed)**: `commands/mod.rs` 仅有 `accounts_create` / `accounts_delete` / `accounts_refresh`；全仓无 `update_credential` / reauth 路径。密钥被吊销或过期后，适配器持续报 `AuthExpired`，账户行停留在错误态。
- **用户后果**: 唯一恢复方式 = 删除账户重建 —— `history_points` 级联清空，30 天用量统计与耗尽预测基线全部丢失。
- **方向**: 新增 `accounts_update_credential` 命令（复用 `persist_new_account` 的 keyring 写入 + 校验 + 回滚模式），行错误态提供"更新密钥"动作；重建不删户。

### P1-2 无额度能力供应商的预期管理缺失
- **Evidence (Observed + Code-confirmed)**: OpenAI 纠正为 L2（仅模型目录）、Qwen 为 L1 后，用户添加账户会**永远**得到空数据，行内只显示"该账户已连接，但接口没有返回可识别的额度字段"。用户无法区分"故障等待恢复"与"该供应商本来就不提供"。
- **方向**: 选择器中对 L1/L2 供应商显示"仅模型目录，无额度查询"说明徽标；该类账户的空态文案按供应商固定为"此供应商不提供密钥级额度查询"而非通用失败措辞。

### P2-3 LAN 同意框不记忆目标
- **Evidence (Code-confirmed)**: `lan_consent_begin` 每次创建都会为该 URL 签发一次性 token；多网关用户每建一个本机账户都要重新确认。
- **方向**: 设置级"已信任的本机地址"列表（显式管理、可撤销），保持一次性令牌的安全语义。

### P2-4 每次首刷失败的原因在设置流程中不可见
- **Evidence (Observed)**: 创建时的首刷失败只进 tracing 与行错误态；表单侧"已连接"提示后用户需自行去概览看错误。
- **方向**: 创建结果内联携带首刷结果摘要（成功/失败原因类别）。

## 3. 后端接口设计与前后端通信

- **正面**: 16 个语义命令、camelCase↔snake_case 由契约测试钉住（`bridge-contract.test.ts`）；`ProviderResponse` 故意不实现 serde，杜绝原始 payload 跨 IPC。
- **P1-4 IPC 错误契约是裸 `String`**: `Result<T, String>` 让 UI 无法按错误类别分支（如 `AuthExpired` → "更新密钥"、`RateLimited` → 显示重试时间）。内部已有 `CommandError` 枚举与稳定 code，只差序列化。
  - **方向**: 错误 DTO 增加 `code` 字段（保持文本面向人），前端按 code 分支。
- **P2-5 `accounts_refresh` 返回全量列表**: 负载随账户数线性增长；>50 账户时可观测。方向：返回 `{ refreshedId, results }` 增量或让前端以事件驱动更新（`quota:updated` 已存在）。
- **P3-6 事件通道缺少类型契约**: `quota:updated`/`notification:enqueue`/morph 事件的 payload 由约定维护，`bridge-contract` 已覆盖名称与 MorphAnchor 字段；后续可把事件 payload 类型集中到 `lib/morph.ts` 式的单一来源。

## 4. 数据流与状态管理

- **正面**: 每视图独立 useState（无全局 store，符合 AGENTS.md）；偏好乐观更新 + 失败回滚；快照集原子替换 + 历史保留窗口清理；双 webview（Bubble/Dashboard）经 `quota:updated` 同步（本会话 F17 修复后实测双向）。
- **P2-7 通知相对时间的全局 tick**: `NotificationSection` 每 30s 更新 `now` state，通知列表整体重渲染。方向：`now` 下推到行组件或把相对时间改为低频 Intl 格式。
- **P3-8 设置面板 `validateInput` 读取 `document.getElementById`**: `SetupForm.tsx:202-207` 绕过 React 受控状态读 DOM。方向：端点值已在 state 中，直接校验 state。
- **P3-9 通知去重与清理队列**（`notifications/mod.rs`）与 `credential_cleanup_pending` 持久队列设计良好（正面确认）。

## 5. 网络安全（聚焦审查）

- **正面（全部 Code-confirmed）**: 传输层单点收口（`HttpClient::send_capped`：Content-Length 预检 + 流式逐块计数上限）；重定向全局拒绝（凭据头不泄漏到第二跳）；显式禁用环境代理（行为确定可测）；敏感头（authorization/x-api-key/cookie/set-cookie）请求日志脱色 + 响应映射直接丢弃；错误文本恒定、不回显 URL；凭据 `Zeroizing` + keyring 原生后端；CSP `default-src 'self'; script-src 'self'`（仅 style unsafe-inline、img data:）；窗口能力最小化；LAN 目标 fail-closed 分类 + 一次性同意令牌。
- **P2-10 企业代理用户被锁死**: `proxy: None` 显式忽略 `HTTP_PROXY`（`networking/mod.rs:27-32`）。这是刻意的确定性设计，但副作用是内网代理用户**完全无法使用**。
  - **方向**: 设置页显式代理输入（用户主动 opt-in 即保留确定性；值仅存内存或 keyring）。
- **info-11**: `style-src 'unsafe-inline'` 为 motion/react 内联样式所需，属常见取舍；如要收紧需迁移到 CSS 变量方案，收益有限。
- **info-12**: 更新器已签名（release 工作流），密钥轮换有密码保护。

## 6. 性能效率

- **正面**: SQLite WAL；每账户单飞 `RefreshLease` 防并发重刷；刷新按 wave 分批；退避 cap 15min + 抖动 + `Retry-After` 优先；响应大小上限流式执行（不缓存全量）。
- **P3-13**: `upsert_snapshot_set` 每次刷新全删全插快照（按账户），快照集小（个位数指标）无碍；若未来指标数增长，可改 diff-upsert。
- **info-14**: 前端 recharts 面积图仅在有数据的 30 天趋势渲染，懒加载于详情页——无感知性能问题。

## 7. 供应商网络请求与接入架构

- **正面**: 三协议适配器（OpenAI 兼容/Anthropic/Gemini）统一 `ProviderAdapter` trait；账户配置经 `FetchContext.config` 传递修正了占位符构造的目标错误（有注释与测试）；Anthropic 限流响应头 → 窗口化快照（官方字段、未知跨度不发明）。
- **P2-15 适配器选择按 `provider_id` 前缀字符串路由**: `refresh_account` 以 `starts_with("anthropic:")` / `"gemini:"` 分派。新增第四协议时需改两处（路由 + 注册），易漂移。方向：在 providers 表的 config 里落 `api_format` 字段并按其路由。
- **info-16**: `FieldMappings.role/metrics` 已支持单响应多指标；manifest 模型与 `to_config` 边界有注册校验兜底。

## 8. UI/UX 品味与最佳实践方向（better-* + frontend-product-ux-audit）

- **正面**: 通知设置披露头（chevron + 状态摘要）与分组（上一批次重构）；QuotaMetric 额度类型自适应卡片；供应商品类胶囊 + 变体列表（popover 语义、外点关闭）；错误聚焦与内联校验（opt-out 原生气泡的取舍有注释）。
- **P2-17 `criticalOnlyHelp` 硬编码"额度低于 10%"**: 阈值可配置（`lowQuotaThreshold`），文案必然漂移。方向：模板化引用当前阈值。
- **P2-18 能力矩阵行数与设置页供应商数双入口**: 矩阵（14 行 mock/31 native）与选择器列表并存，用户在两处看到不同供应商集合（矩阵含"规划中"行）。方向：矩阵作为选择器的"能力说明"折叠视图而非平行列表。
- **P3-19 历史页 30 天趋势在空数据下只显示 `noData`**：可与耗尽预测 hint 合并为"从今天开始积累"的正向空态（已有部分文案）。
- **P3-20 Bubble 尺寸 56×56 对高 DPI 多显示器场景的坐标换算已处理（`persist_bubble_position` snap-to-edge）**，多显示器运行时验证仍受硬件限制（沿用既有边界记录）。

## 9. 路线图（按影响排序）

| 优先级 | 事项 | 涉及 | 验收 |
|---|---|---|---|
| P1 | 密钥更新命令 + 行内"更新密钥"动作 | `commands/mod.rs`、`AccountRow` | 失效密钥不删户即可恢复；历史保留 |
| P1 | IPC 错误 DTO 携带 `code` | commands + tauri.ts + UI 分支 | AuthExpired 可编程识别 |
| P1 | L1/L2 供应商预期文案（选择器 + 空态） | SetupForm、copy、AccountRow | OpenAI 账户空数据不再像故障 |
| P2 | 显式代理设置 | settings 表 + HttpClientConfig | 代理用户可用且默认行为不变 |
| P2 | LAN 目标信任列表 | settings + consent 流程 | 二次创建不再弹窗且可撤销 |
| P2 | `criticalOnlyHelp` 阈值模板化 | copy | 改阈值后文案同步 |
| P3 | api_format 入库替代前缀路由 | storage + scheduler | 新协议零改路由 |
| P3 | 通知 now 下推 / 受控校验 | NotificationSection / SetupForm | 减少重渲染与 DOM 读取 |

## 10. 验证矩阵与诚实边界

| 维度 | 状态 |
|---|---|
| 代码质量/接口/数据流/状态 | Code-confirmed（逐文件阅读 + 会话内实机操作） |
| 网络安全 | Code-confirmed（聚焦审查，非完整渗透审计；未做模糊测试/依赖 CVE 扫描） |
| 供应商配置 UX | Observed（本会话多次原生驱动该流程）+ Code-confirmed |
| UI/UX 方向 | 部分渲染验证（原生截图），窄窗/200% 缩放由 e2e 覆盖 |
| macOS/多显示器 | 维持既有未验证边界 |

## 11. 依赖审计记录（2026-11-05）

每周 `Dependency Audit` 工作流在 v1.5.0 上报出两个失败：Rust 侧 `zip 4.6.1`
高危（RUSTSEC-2026-0429，`ZipArchive::extract` 对大尺寸符号链接条目中止进程）
与前端侧 `decode-uri-component <=0.5.0` 高危（GHSA-qr28-fpjm-wq7f，百分号解码
二次复杂度 DoS）。

### 已修复

| 项 | 修复 | 说明 |
|---|---|---|
| zip 高危 + 三条旧忽略 | `tauri-plugin-updater` 2.12.0→2.14.1（首个依赖 `zip ^9` 的 2.x），连带 tauri 全家 2.11.6→2.12.5，zip →9.2.2 | `.cargo/audit.toml` 三条 zip 忽略按其自述复评策略移除；quick-xml 两条保留（plist 仍钉 quick-xml 0.38） |
| MSRV | `rust-version` 1.88→1.90（README 同步） | tauri 2.12 全家声明 1.90；v1.4.0 为 `time` 链提 1.88 已有先例 |
| decode-uri-component | 0.5.0→0.5.1 | `query-string@9.5.1` 本就允许 `^0.5.0`，范围内浮动 |
| `@tauri-apps/api` | 2.11.1→2.12.4 | tauri 2.12.5 升级时漏掉的配套项：dev 启动时报主/次版本不匹配警告 |
| `@lobehub/icons` | 5.23.0→5.23.1 | 5.x 线内末版 |
| katex 低危（GHSA-238p-pmpm-9mq7） | `pnpm-workspace.yaml` overrides 统一到 `^0.18.10` | `micromark-extension-math@3.1.0`（上游最新）仍钉 `katex ^0.16.0`，范围外 override 是唯一路径；katex 从不进入打包产物（`dist/assets` 检查为空），override 后树中只余单份 0.18.10 |
| async-executor yanked | 1.14.0→1.14.1 | 锁文件引用已撤回版本，补丁版一条命令可达 |
| dev 启动间歇挂起 | `tauri.conf.json` devUrl/beforeDevCommand 对齐 `127.0.0.1` | Node 17+ 对 `localhost` 的双栈解析顺序不定，vite 可能只绑 `[::1]` 而 tauri 侧连 IPv4（SYN_SENT 挂起）；e2e 一直显式用 `127.0.0.1` 所以从未触发 |
| 本地 e2e 间歇挂/败 | 无代码问题（干净 HEAD 复现） | 多次 `pnpm install` 使 vite optimizeDeps 缓存失效后，Windows 冷优化首载会拖过 global-setup 的 `networkidle` 30s 或拖慢 dock 点击用例；预热一次真实加载即恢复。CI 干净环境不受影响。遇到时先预热再怀疑代码 |

### 留存（明确留到下一阶段或上游）

| 项 | 去留判定 | 理由 |
|---|---|---|
| 6 条 warning（proc-macro-error、unsafe-libyaml、zbus_names/zvariant/zvariant_derive、glib unsound） | 留 | `cargo audit` 退出码 0，不失败；全部是 Linux 目标或构建工具链传递依赖，修复需要 notify-rust / tauri / GTK 绑定上游发版；Windows 构建不编译这些目标 |
| `@lobehub/icons` 6.x（自含 `katex ^0.18` 且不再依赖 ui） | 留到下一阶段 | 主版本跳跃；31 个内置供应商品牌图标经它渲染，需要逐图标视觉回归后才可升级，不宜混入依赖安全热修 |
| 悬浮球展开/折叠实机复验 | 本轮完成 | wry 0.55→0.57、window-vibrancy 0.6→0.8、tray-icon 0.24→0.25 全部是窗口栈，恰是 morph 路径触碰的层，属"应验尽验" |

### 验证

`cargo audit` / `pnpm audit --prod --audit-level high` 均 0 漏洞退出；
clippy `-D warnings`、cargo test、typecheck、vitest、e2e、`pnpm build`
（bundle 不含 katex 复查）全绿；实机冒烟见 §10。
