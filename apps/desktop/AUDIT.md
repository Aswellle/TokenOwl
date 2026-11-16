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

`cargo audit` 0 漏洞退出（仅余 6 条上游阻断的 allowed warning）/ `pnpm audit --prod --audit-level high` 静默退出；
cargo fmt --check 干净、cargo test 402 passed（14 个套件）、clippy `--all-targets --all-features -D warnings` 0 错误；
pnpm typecheck 0、vitest 194 passed（23 文件）、`pnpm build` 成功、Playwright 127 个用例按 CI 配置（retries=1）运行 126 passed + 1 flaky（dock 键盘用例，重试即过，属 §11 记录的 Windows 冷启动抖动）。
真实桌面冒烟仍按 §10 清单执行；本轮新增功能只经 webview e2e 与 loopback 集成测试验证。

## 12. 功能闭环修复与能力升级（2026-11-16）

依据《TokenOwl_Agent_Development_Plan.md》执行的一轮完整实现。审计基准为本轮开始时的 main；所有结论以当前代码为准。

### 已实现（按提交）

| 提交 | 内容 |
|---|---|
| 1e8c793 | model-only L2 供应商经 models 探测给出健康结论（OpenAI-compatible 适配器 + UI 文案 + 11 个 mock-server 集成测试） |
| 3f7af51 | reset endpoint 真实请求/解析/合并：rfc3339 / unix_seconds / unix_millis 三种声明格式，窗口绑定或账户级独立卡片，失败不抹除兄弟数据 |
| 1a4848a / 022fbf7 | 候选 Key 先验证后提交（一次性候选引用 + 三协议 probe + 四态结果 + UI 仅验证成功才关对话框）；Windows 测试二进制补挂 Common-Controls 清单（comctl32 TaskDialogIndirect 入口点问题根因） |
| 3218193 | 历史保留期 90→365 天，与 26 周热力图对齐；保留边界精确测试 |
| 92d7888 | RefreshDiagnostics DTO（稳定码/HTTP 状态/端点归属/时间戳）；首刷区分“已保存/已验证”；错误类别逐类文案 |
| 952cbb1 / 632152d | README 供应商表由清单生成 + 漂移测试；修正 29→31、OpenAI L2 / OpenRouter L3；capability 增加 lastVerifiedAt 字段 |
| bb7316c | 后台刷新节奏遵循各供应商 default_refresh（60 秒下限、全局节奏上限） |
| （并发提交） | 独立端点每账户最多 3 路并发、按声明顺序合并；400ms×2 延迟夹具证明总耗时 ≈ 单端点而非串行 |
| 8a3ceb4 | 本地 Agent 使用量（Claude Code 已实测 schema；Codex 按文档格式实现并标注未核验）：opt-in、只读、限定目录、幂等去重（migration 0005）、与远端额度分域 |
| 561a411 / da3b906 | 历史导出 CSV/JSON 到应用数据 exports 目录（RFC 4180 转义；不含凭据；目录只保留最近 20 个导出文件，从不删除非导出文件） |
| 30bedb8 | rusqlite 0.32→0.43（libsqlite3-sys 0.40）修复 RUSTSEC-2026-0459 捆绑 SQLite CVE；u64 绑定/读取点改为 i64 转换；rustix 1.1.6 清除其 soundness 警告 |
| 97d23f3 | Codex 解析改为 `last_token_usage` 逐轮增量（累计 total 会重复计数），支持 cumulative-only 差分与 `turn_context` 模型；两来源增加文件级增量 cursor（路径哈希 + head/anchor 指纹，路径不入库），半行等待换行后再读 |
| 328f306 | 每账户刷新计划：provider 默认间隔、用户 1/5/10/15/30 分钟上限、失败退避与可选自适应（低额度/临近重置最多减半，2 小时空闲最多加倍）；账户行/详情显示下次计划刷新 |
| a48e886 | 社区清单 `schemaVersion`（缺省=1，未来版本拒绝）、仅 `api_key`、未知字段与映射边界硬校验；选择器显示来源文件/schema/最近请求状态；内置与社区共享 conformance；新增离线 Manifest 实验台 |
| 310ba74 | 账户配置备份（仅 Provider/名称/地址/协议/端点，不含 Key 或 credential ref）；导入预览不创建，逐条重输 Key 走正常创建/验证；导出目录改为跨类型统一按时间裁剪 |
| 43ff3ad | 每账户端点测试（按路径去重、有界并发、稳定 outcome/HTTP/延迟、无响应正文）；本地脱敏诊断包先预览后导出，token 只覆盖时间戳之外的稳定内容，自定义端点 host/路径/查询一律不出现 |
| （本轮收尾） | `lastVerifiedAt` 格式/未来日期校验；README 增加“API 核验 / 契约覆盖”两列；Provider 详情显示本地最近成功时间；dock 键盘 e2e 等待偏好加载以消除冷启动抖动 |

### 已知限制（如实记录）

- Codex 解析器仍未在真实安装上核验（本机未安装 Codex CLI）。本轮按公开 rollout 格式修正了逐轮增量、模型归属与断点续扫，并以 fixture 覆盖；UI 来源卡片继续标注“尚未核验”。
- `lastVerifiedAt` 只表示一次真实 API 核验，本轮没有可用密钥对 31 家供应商逐一核验，因此没有回填日期。README 用“API 核验 = 未声明”与“契约覆盖 = 已覆盖”两列如实区分；Provider 详情的“最近成功更新”来自本机刷新，不冒充供应商核验。
- OAuth / device flow 与浏览器会话来源仍未实现。为避免能力冒充，社区/内置清单的 `auth` 现在只接受 `api_key`，未知认证方式在加载时拒绝；UI 没有不可执行的登录入口。
- 本轮未做真实供应商联调（不引入真实密钥到 CI）；models probe / reset / key rotation / endpoint diagnostics 均以 loopback mock server 按线协议验证。
- e2e 只覆盖 webview 层；托盘、浮球、keyring、通知权限的原生行为仍按 AGENTS.md 的手动清单验证。macOS 仍未在真实硬件验证。

### 本轮验证（2026-11-16，Windows）

| 门禁 | 命令 | 结果 |
|---|---|---|
| Rust 测试 | `cargo test`（来自 `apps/desktop/src-tauri`） | **441 passed**（382 lib + 59 integration），0 failed |
| Rust lint | `cargo +1.100.0 clippy --all-targets --all-features -- -D warnings` | 0 错误、0 警告 |
| Rust 格式 | `cargo +1.100.0 fmt --all -- --check` | 干净 |
| MSRV | `cargo +1.90 check --all-targets` | 通过 |
| 前端类型 | `pnpm typecheck` | 0 错误 |
| 前端单测 | `pnpm test` | **211 passed / 28 files** |
| 前端构建 | `pnpm build` | 成功（仅有既存的 >500 kB chunk 提示） |
| E2E | `pnpm e2e`（本地 retries=0） | **132 passed**，无 flaky（10.4 min） |
| Rust 依赖审计 | `cargo audit` | 0 vulnerability，6 条 allowed warning（均已在 §11 记录） |
| 前端依赖审计 | `pnpm audit --prod --audit-level high` | No known vulnerabilities |

说明：E2E 为 webview/mock 层，不等同于原生壳验证；本轮未新增 macOS 实机结论。`lastVerifiedAt` 没有填造日期；真实供应商联调仍需可用密钥，不在本轮自动化范围内。

## 13. 实机端到端联调与可用性修复（2026-11-16）

在 §12 的能力闭环基础上，补齐“真实进程 + 真实 IPC/存储/凭据/网络 + 真实窗口”的自动化实机层，并按实机截图与可访问性审计修复产品问题。实机层不进入 CI（需要 GUI 与 WebView2），作为发版前 Windows 验证使用。

### 新增测试资产

| 文件 | 内容 |
|---|---|
| `playwright.native.config.ts` + `e2e-native/native-app.ts` | 启动真实 debug 二进制，经 WebView2 CDP 连接主窗口与 Bubble 窗口；独立 app-data/keyring/home，Win32 窗口枚举与缩放 |
| `e2e-native/fixture-provider.ts` | 可切换 windowed/low/balance/auth/5xx/429/非 JSON 的 loopback 供应商，记录实际请求路径与 Key |
| `e2e-native/*.spec.ts`（11 文件 / 12 用例） | 创建/刷新/端点测试、密钥轮换、降级、备份/导入、诊断、通知、本地用量、社区清单、重启持久化、窗口交接、可访问性、视觉取证 |
| `e2e/flows.spec.ts`（10 用例） | CI 内覆盖密钥轮换三态、LAN 信任移除、本地用量 opt-in、代理保存/清除、诊断 token 过期、部分账户导出、视图滚动复位 |
| `src-tauri/tests/proxy_transport.rs` | 显式代理确实收到绝对 URI，源站未被直连 |
| `src-tauri/src/commands/mod.rs` reset 解析测试 | 清单声明的 reset 端点通过账户解析，不再中途丢弃 |
| `e2e-native/README.md` | 运行方式、隔离变量、覆盖边界与清理策略 |

### 修复

| # | 修复 |
|---|---|
| F20 | 单账户刷新失败后重新读取账户列表，保留后端持久化的错误码/诊断，恢复“更新密钥”入口 |
| F21 | `AccountEndpoints`/创建持久化贯通清单 `reset` 端点 |
| F22 | 主按钮改用满足 4.5:1 的专用底色（含深色主题值） |
| F23 | Dock 收起/展开控制高度 18→24px |
| F24 | 首跑空态标题 h3→h2 |
| F25 | 视图切换时内容滚动区回到顶部 |
| F26 | 历史热力图加载后滚到最新（最右）端 |
| F27 | Bubble 供应商标记加浅色底片，三环时收紧内边距 |
| F28 | Vitest 排除 e2e-native；新增 debug-only `TOKENOWL_TEST_HOME`；修复两个高负载竞态单测 |

### 验证（2026-11-16，Windows 11 22631，150% DPI）

| 门禁 | 命令 | 结果 |
|---|---|---|
| Rust 测试 | `cargo test` | **443 passed**（383 lib + 60 integration），0 failed |
| Rust lint | `cargo +1.100.0 clippy --all-targets --all-features -- -D warnings` | 0 错误、0 警告 |
| Rust 格式 | `cargo +1.100.0 fmt --all -- --check` | 干净 |
| MSRV | `cargo +1.90 check --all-targets` | 通过 |
| 前端类型 | `pnpm typecheck` | 0 错误 |
| 前端单测 | `pnpm test` | **211 passed / 28 files** |
| 前端构建 | `pnpm build` | 成功 |
| 浏览器 E2E | `pnpm e2e`（retries=0） | **142 passed**（16.0 min），0 flaky |
| 实机 E2E | `pnpm e2e:native` | **12 passed**（3.0 min） |
| 依赖审计 | `cargo audit` / `pnpm audit --prod --audit-level high` | 0 vulnerability；6 条既有 allowed warning |

实机断言覆盖四个已填充视图的 landmark、标题层级、按钮/控件名称、重复 id、24px 目标、可对比背景的对比度、焦点环、对话框 Tab 陷阱与 560×480 reflow；截图位于 `verify-shots/native-e2e-2026-11/`（未跟踪，供人工复核）。

### 边界

- 托盘点击、物理全局热键、OS 通知横幅、更新器下载/安装、多显示器、macOS 仍未自动化或未验证。
- Codex 解析器仍未在真实安装上核验。
- 实机套件使用 debug 构建；release 安装包仍按发版冒烟清单单独验证。
