# TokenOwl Implementation Report

## Round 1 — 健康分级阈值（P1）

日期：2026-10-11 | 基线 commit：ca3fb7b（main）| 平台：Windows 11 x64

## 1. 本轮目标与依据

`docs/IMPLEMENTATION-GAP.md` 中唯一完全可离线实施的 P1/Missing 项：**健康分级阈值**。
PRD §5.1 定义综合状态：`Attention = 任一窗口剩余 < 25%`、`Critical = 任一关键窗口剩余 < 10%`；
`docs/07` Phase 6（Polish）与 P1 Notifications 都以此为前置。全量阅读 docs/ 17 份文档后确认
前端渲染链路（chip 样式、`overallHealth`、attention 筛选）已就绪但后端从不产生
`attention`/`critical` 状态，本轮回补该缺口。

## 2. 需求完成矩阵

| 要求 | 来源 | 结果 |
|---|---|---|
| Attention/Critical 阈值分级 | PRD §5.1 / GAP P1 | ✅ `domain/health.rs`，严格边界（恰为 25%/10% 不触发下一级） |
| 分级在 Rust domain 实现（UI 无关、可测试） | 02 / 07 | ✅ 纯函数 + 7 个边界单测 |
| unknown ≠ zero | 总规范 / 04 | ✅ 无 limit 或不可信快照返回 `None`，永不推断耗尽 |
| 账户状态透传到 UI | 04 §12（ViewModel 语义转换在 Rust） | ✅ `account_view_to_model` 输出 8 种 `AccountHealth` |
| 状态不只靠颜色 | 05 §2.2 | ✅ 沿用既有 icon + 文案 chip（`health.attention`=需要关注、`health.critical`=额度紧张） |
| 条形/ring 色调与状态阈值一致 | 05 §9 / GAP"先确认阈值" | ✅ 前端 `ratioTone` 从 15%/40% 统一为 PRD 10%/25% |
| 边界与降级测试 | 06 §9 / 07 | ✅ Rust 13 个 + 前端 10 个新测试 |

## 3. 代码变更

- **新增 `apps/desktop/src-tauri/src/domain/health.rs`**：`HealthGrade`（Healthy<Attention<Critical）、
  `remaining_ratio`（优先 remaining/limit，回退 (limit−used)/limit；limit 非法或数值非有限 → `None`；
  结果钳制到 0..=1，坏数据只能软化不能加重分级）、`grade_for_ratio`、`worst_grade`；7 个嵌入单测。
- **`src-tauri/src/domain/mod.rs`**：注册 `health` 模块。
- **`src-tauri/src/commands/mod.rs`**：新增 `account_data_grade`（仅 `Available`/`Partial` 快照参与分级）；
  `account_view_to_model` 状态优先级改为 `error` → `unavailable` → `critical` → `attention` → `stale` → `healthy`；
  新增 6 个状态路径单测。
- **`apps/desktop/src/lib/format.ts`**：`ratioTone` 阈值统一为 PRD 10%/25%（与 Rust `domain/health.rs` 注释互指）。
- **新增 `apps/desktop/src/test/health.test.ts`**：阈值边界、ratio 推导、pressure、overallHealth 传播。

未改动：调度器、存储 schema、凭据、网络层、既有组件渲染逻辑。

## 4. 行为变化

- 额度剩余 <10% 的账户现在显示"额度紧张"（critical），<25% 显示"需要关注"（attention），
  悬浮球 ring 与仪表盘筛选/计数随之生效（此前这些分支不可达）。
- 数值分级优先于 stale（stale 的紧张额度仍需醒目）；`error`/`unavailable` 语义不变。
- 条形颜色在 15–25% 区间从 healthy 变为 attention、10–15% 从 attention 变为 critical（阈值统一）。

## 5. 测试与构建（全部通过）

| 检查 | 结果 |
|---|---|
| `cargo fmt --all -- --check` | ✅ |
| `cargo clippy --all-targets --all-features -- -D warnings` | ✅ 0 警告 |
| `cargo test` | ✅ 125（118 单元 + 7 fixture；含 13 个新测试） |
| `pnpm typecheck` | ✅ |
| `pnpm test` | ✅ 22/22（12 既有 + 10 新增） |
| `pnpm build` | ✅（既有 chunk-size 提示，非新增） |

## 6. Diff 自审（docs/07 §5）

1. **修改文件**：上节 5 个代码文件 + 3 个文档。
2. **为什么**：闭合 GAP P1 Missing 的健康分级；其余为该功能的最小接线。
3. **删除**：无（仅替换 `account_view_to_model` 的状态推导与 `ratioTone` 常量）。
4. **行为变化**：见第 4 节；无破坏性——既有 118+7 Rust 与 12 前端测试原样通过。
5. **文档未实现项**：reset 邻近度分级、通知链路、多端点合并、迁移备份（见 IMPLEMENTATION-GAP）。
6. **部分支持**：分级仅用剩余比例；重置临近度留给通知阶段的 `quota:reset-soon`。
7. **Windows/macOS**：Windows 已验证；macOS 未验证（本轮未触碰平台相关代码）。
8. **schema 变化**：无，无 migration。
9. **secret 风险**：无；分级只消费归一化数值。
10. **架构耦合**：无新增；分级在 domain 层，UI 仅消费状态字符串。

## 7. 已知缺陷与限制

- 未经真实低额度账户的视觉走查（无真实凭据）；渲染组件/样式为既有已验证路径。
- critical/attention 基于最后已知数值（含 stale/expired 快照），数据年龄由新鲜度 chip 单独呈现。
- `FreshnessPolicy.stale_after` 未驱动刷新间隔（既有限制，未变）。

## 8. 下一阶段建议

1. SQLite migrations 目录结构（版本化 + 备份 + integrity 检查）。
2. Provider 多端点合并（balance+usage+quota）。
3. 通知触发链路（阈值已就绪；需评估 `tauri-plugin-notification` 权限边界）。

## 9. 可发布性

本轮无 schema/发布链路变化，不改变 release 状态："release foundation configured, publishing
blocked"（见 docs/11），macOS 与签名 release 演练仍未完成。变更未提交，等待确认后按
`feat(domain): add health grading thresholds` 单一职责提交。

## Round 2 — SQLite 迁移框架：版本化 + 备份 + 完整性检查（P1）

日期：2026-10-11 | 基线：Round 1 工作区 | 平台：Windows 11 x64

### 目标与依据

`IMPLEMENTATION-GAP.md` 中下一项可离线完成的 P1/Partial：**SQLite migrations 缺少升级备份与
integrity 检查**（原实现只有内联 v1 schema + `user_version`）。依据 docs/06 §18：迁移前检查
完整性、升级失败保留旧库并生成 backup、损坏库进入 safe mode；docs/11 要求更新不破坏本地
SQLite。

### 需求完成矩阵

| 要求 | 来源 | 结果 |
|---|---|---|
| 版本化迁移结构（001_initial …） | 06 §18 / STATUS | ✅ `storage/migrations/` 目录 + `0001_initial_schema.sql`（`include_str!` 嵌入）+ 有序 `MIGRATIONS` 表 |
| 每次迁移前检查 integrity | 06 §18 | ✅ 迁移前后各一次 `PRAGMA integrity_check`；`SQLITE_CORRUPT` 中断归一化为 `Integrity` 错误 |
| 升级备份、失败保留旧库 | 06 §18 / GAP | ✅ 对含数据的库先 `VACUUM INTO` 生成 `*.pre-migration.bak`（仅保留最近一代）；每个迁移独立事务，失败自动回滚 |
| 损坏库拒绝打开（safe mode） | 06 §18 / 11 | ✅ integrity 失败返回错误，不触碰数据、不产生备份 |
| 新版本 schema 拒绝打开 | 06 §11 | ✅ `user_version` 高于支持版本 → `UnsupportedSchema`（原有行为保留） |
| fresh 库跳过备份 | — | ✅ 空库无 `sqlite_master` 用户表时不产生备份文件 |

### 代码变更

- **新增 `src-tauri/src/storage/migrations/mod.rs`**：`Migration` 结构、`MIGRATIONS` 有序表、
  `SCHEMA_VERSION`（= 最后一项版本，编译期推导）、`apply_migrations_with`（integrity → 备份 →
  逐版本事务 → 复查），`integrity_check` / `has_user_tables` / `create_backup` / `apply_one`；
  6 个嵌入测试（fresh 无备份、升级备份+数据保留、损坏拒绝、新版本拒绝、失败回滚、空表 noop）。
- **新增 `0001_initial_schema.sql`**：原 `SCHEMA_V1` 原样迁移为编号 SQL 文件。
- **`storage/mod.rs`**：删除内联 `SCHEMA_V1`/`SCHEMA_VERSION` 常量与旧 `migrate()`；`open()` 把
  数据库路径传入迁移器（内存库传 `None` 跳过备份）；`StorageError` 新增 `Integrity`、
  `Migration { version, name, source }`、`Backup(io::Error)` 三个变体。

未改动：表结构、WAL/外键 pragma、上层 repository API、前端。

### 行为变化

- 打开**已有数据**的数据库且存在待执行迁移时：先 integrity 检查并生成备份，再逐版本升级。
- 当前 schema 仍为 v1，因此现有用户库在本次变更下不会触发备份或迁移（行为不变）；
  该框架为 v2+ 迁移（如 provider manifest、usage history 表）提供安全升级路径。

### 测试与构建（全部通过）

| 检查 | 结果 |
|---|---|
| `cargo fmt --all -- --check` | ✅ |
| `cargo clippy --all-targets --all-features -- -D warnings` | ✅ 0 警告 |
| `cargo test` | ✅ 131（124 单元 + 7 fixture；含 6 个新迁移测试） |
| `pnpm typecheck` / `pnpm test` / `pnpm build` | ✅ 22/22，构建通过（前端无改动） |

### Diff 自审（docs/07 §5）

1. **修改文件**：3 个新文件（migrations/mod.rs、0001_initial_schema.sql）+ storage/mod.rs + 3 个文档。
2. **为什么**：闭合 GAP P1 SQLite 迁移项；schema 文件化是版本化结构的一部分。
3. **删除**：内联 `SCHEMA_V1` 字符串与旧 `migrate()`（逻辑等价移入迁移器，SQL 逐字保留）。
4. **行为变化**：仅"有待执行迁移的已有数据库"路径新增备份+校验；v1 用户行为不变。
5. **文档未实现项**：自动整库还原未实现——按 docs/06"回滚**或** safe mode"取后者（事务回滚 +
   拒绝打开 + 磁盘备份供手动恢复），已在 GAP 偏差栏注明。
6. **部分支持**：备份仅保留最近一代（重复升级会覆盖上一代备份）。
7. **Windows/macOS**：Windows 已验证；macOS 未验证（未触碰平台代码）。
8. **schema 变化**：无（v1 结构原样）；**migration 机制本身有变化**，含备份与完整性路径。
9. **secret 风险**：无。备份文件是原库的完整拷贝（同样只含 `credential_ref`，无 secret），
   写在应用数据目录旁，与数据库同权限。
10. **架构耦合**：无新增；迁移器是 storage 子模块，上层不可见。

### 已知限制与下一阶段

- 备份保留最近一代；自动还原未实现（见上）。
- `integrity_check` 在大库上是一次全表扫描，仅在有待执行迁移时执行，正常启动零开销。
- 下一阶段建议：Provider 多端点合并 → 通知触发链路（依赖本轮阈值 + v2 迁移可顺带扩展 schema）。

## Round 3 — Provider 多端点合并 + UI/UX 匹配重构（P1）

日期：2026-10-11 | 基线：Round 2 工作区 | 平台：Windows 11 x64

### 目标与依据

`IMPLEMENTATION-GAP.md` 中 Generic OpenAI-compatible 行的既定下一步：**quota/usage/balance
多端点合并**（原实现一个账户只请求单一 endpoint）。同步要求：UI/UX 重构为匹配多桶业务数据，
并调用 design-review skill 按最佳实践清单做渲染审计。

### 需求完成矩阵

| 要求 | 来源 | 结果 |
|---|---|---|
| 多端点合并 | GAP P1 / docs/03 | ✅ 适配器按 quota→usage→balance 顺序请求全部已配置端点，按路径去重 |
| 部分失败隔离 | 总规范失败不变量 | ✅ 失败端点生成 `Error` 指标快照（不伪造数值），其余端点继续；全部失败返回首个错误走 stale fallback |
| 无额度字段的响应 | docs/09 规则 | ✅ `parse_openai_compatible_lenient` 返回 `Ok(None)`，静默跳过；严格解析器行为不变 |
| 前端配置三端点 | docs/03 §12 最小配置 | ✅ 表单新增用量/余额接口（可选），默认 `/usage` 仅在无其他端点时生效 |
| 多桶 UI 匹配 | docs/05 §6 | ✅ 余额（仅剩余值）/用量（仅已用）指标按存在的字段渲染，无伪造百分比；失败指标显示"该指标读取失败" |
| 渲染审计 | 用户要求（skill） | ✅ design-review 清单 + 浏览器实测（1280/375）；发现并修复列表 DOM 缺陷 |

### 代码变更

- **`domain/provider.rs`**：新增 `parse_openai_compatible_lenient`（严格版 `parse_openai_compatible_fixture`
  改为在其上包装，行为不变）。
- **`providers/builtin/openai.rs`**：`fetch` 重写为多端点循环 + `EndpointOutcome`（Snapshots/
  Unrecognized/Failed）+ 纯函数 `merge_endpoint_outcomes`（可离线测试）；HTTP 读取抽取为 `read_response`；
  新增 5 个合并/去重/失败语义测试。
- **`commands/mod.rs`**：输入结构新增 `usage_endpoint`/`balance_endpoint`；`validate_new_account`
  返回 `AccountEndpoints`；默认端点规则调整；新增端点校验测试。
- **前端**：`types`（两个可选端点字段）、`copy.ts`（三端点文案 + `metrics` 本地化映射）、
  `App.tsx`（表单字段、createAccount 透传、`QuotaMetric` 指标名本地化）、`mock.ts`（多端点演示账户）。
- **渲染审计修复（App.tsx）**：账户行此前直接渲染在 `<section>` 下（`li` 脱离列表容器 → 无效 HTML
  + 原生 disc 圆点泄漏）；改为 `<ul class="accounts">` 包裹、`AnimatePresence` 内移保持逐项退出动画。
  审计确认：删除流程端到端正常，全局状态随账户删除联动更新。

### 测试与构建（全部通过）

| 检查 | 结果 |
|---|---|
| `cargo fmt --all -- --check` / `clippy --all-targets --all-features -D warnings` | ✅ 0 警告 |
| `cargo test` | ✅ 137（130 单元 + 7 fixture；含 6 个多端点新测试） |
| `pnpm typecheck` / `pnpm test` / `pnpm build` | ✅ 22/22，构建通过 |
| 浏览器渲染审计 | ✅ 1280/375 双宽度、暗色主题、mock 四账户（含多端点/部分/错误态）、删除交互 |

### Diff 自审（docs/07 §5）

1. **修改文件**：Rust 3 + 前端 5 + 文档 3。
2. **为什么**：闭合 GAP 多端点项；UI 随业务数据形态扩展并修复审计发现的 DOM 缺陷。
3. **删除**：单端点 fetch 路径（被多端点循环取代，行为兼容单端点账户）。
4. **行为变化**：配置了多端点的账户现在一次刷新合并全部指标；单端点账户行为不变。
5. **文档未实现项**：JSONPath/JMESPath 映射、LAN 授权提示（GAP 已注明归属 manifest 增强）。
6. **部分支持**：错误指标不保留旧数值（快照集合整体替换），新鲜度与错误态单独可见。
7. **Windows/macOS**：Windows 验证（含浏览器渲染审计）；macOS 未验证。
8. **schema 变化**：无（快照 JSON 复用现有表；v2 迁移留给历史序列）。
9. **secret 风险**：无新增——端点仍是相对路径校验，凭据仍只在请求时解析。
10. **架构耦合**：无新增；合并逻辑是适配器内纯函数。

### 已知限制与下一阶段

- 错误指标的旧数值随集合替换丢失（错误态显式可见为补偿）；per-metric 保留旧值需要指标级合并，留待历史序列设计。
- 下一阶段：通知触发链路（需权限决策）→ 历史趋势（v2 迁移）。

## Round 4 — 通知触发链路：低额度 / 重置临近 / 认证过期 → OS 通知（P1）

日期：2026-10-11 | 基线：Round 3 工作区 | 平台：Windows 11 x64

### 目标与依据

`IMPLEMENTATION-GAP.md` 中下一项 P1：**通知触发链路未实现**（原 `notifications/` 模块只有孤立
结构体，未接存储、刷新链路与系统通知）。依据 docs/01 §12（触发条件、Windows/macOS 通知、
静默模式、只提醒 Critical、"可选，不打扰为原则"）、docs/02 §7（SQLite 保存 Notification
history）、docs/04 §11（`notification:enqueue` 事件词汇）。

**权限边界决策（用户确认）**：OS 通知仅由 Rust 端触发——新增 `tauri-plugin-notification`
依赖并在 Rust 注册，但 `capabilities/default.json` **零改动**，webview 不持有任何通知权限，
只能通过语义命令读历史/标已读/改偏好。符合最小权限原则；webview 侧代码被注入也无法绕过
后端节流直接弹系统通知。

### 需求完成矩阵

| 要求 | 来源 | 结果 |
|---|---|---|
| 低额度提醒 | 01 §12 | ✅ 最差可信指标剩余比例 < 用户阈值（默认 20%，`NotificationPreferences` 可配 1–99）；< 10%（`CRITICAL_RATIO`）升级 CRITICAL，否则 WARNING；仅统计 `Available`/`Partial` 快照，未知不补零 |
| Balance < 用户设定金额 | 01 §12 | ✅ `metric_id == "balance"`（多端点合并时已标注）或 metric_name 为 balance 的快照，remaining < 阈值（默认关闭，`None`）触发 WARNING |
| reset 即将发生 | 01 §12 | ✅ `window.reset_at` → 快照 `reset_at` → `ends_at` 依次回退，RFC3339 解析（chrono），默认提前 30 分钟（300s–24h 可配）；dedup key 含重置时间戳，每周期只提醒一次 |
| 认证过期 | 01 §12 | ✅ 刷新失败且 error_code 为 `AuthRequired`/`AuthExpired` → CRITICAL，本周期取代数值类规则（避免在陈旧值上叠加噪音） |
| Provider 长时间失去连接 | 01 §12 | ✅ 连续 ≥3 次网络类失败（NetworkUnavailable/Timeout/TlsError/ProviderUnavailable）→ WARNING |
| 静默模式 / 只提醒 Critical | 01 §12 | ✅ `silent_mode` 仍记历史不弹横幅；`critical_only` 仅 CRITICAL 弹横幅；`enabled=false` 完全不评估 |
| Windows/macOS Notification | 01 §12 / 05 §14 | ✅ `tauri-plugin-notification` Rust API，派发前经插件授权面（`request_permission`）守卫（Windows 已随构建验证编译链接；横幅实机弹出待安装版环境） |
| SQLite 通知历史 | 02 §7 | ✅ 迁移 v2 `notifications` 表：24h 去重冷却、30 天保留 + 200 条上限修剪、账户删除 `ON DELETE SET NULL` 保留历史、外键开启 |
| `notification:enqueue` 事件 | 04 §11 | ✅ 每条实际入队（通过去重）的通知 emit 一次，前端监听刷新列表 |
| 不打扰原则 | 01 §12 | ✅ 每条件每冷却窗口至多一条；认证过期当周期独占；OS 派发失败仅记日志不影响刷新 |

### 代码变更

- **`notifications/mod.rs`（重写）**：`NotificationKind`/`NotificationSeverity`（serde 协议
  枚举 + DB 字符串互转）、`Notification`（含 `dedup_key`）、`NotificationPreferences`
  （serde default + `validate()` 边界校验）、纯函数 `evaluate_account_notifications(TriggerContext)`
  拆为 5 个规则助手（auth/unreachable/low_quota/balance/reset）、`enqueue_new`（去重入队 →
  按 `NotificationSink` 派发 → 修剪）、`NotificationSink` trait + `LogSink`（测试）+
  `OsNotificationSink`（AppHandle → 插件 Rust API，派发前调用插件 `request_permission`
  守卫：非 `Granted` 跳过系统派发只记日志）；14 个评估测试 + 4 个入队测试。
- **`storage/migrations/0002_notifications.sql` + `MIGRATIONS` 追加**：通知历史表（两个索引），
  版本升至 2，老库升级自动走 Round 2 的备份 + 完整性检查路径。
- **`storage/mod.rs`**：`NotificationRecord`（存储层自有类型，不依赖 notifications 模块）+
  `insert_notification`（冷却查询 + 事务插入）/`notifications`/`mark_notifications_read`/
  `mark_all_notifications_read`/`prune_notifications`；4 个存储测试（含外键 SET NULL 行为）。
- **`commands/mod.rs`**：`notifications_list`/`notifications_mark_read`/
  `notification_preferences_get`/`notification_preferences_set` 4 个命令 +
  `NotificationViewModel`；`refresh_accounts` 增加 `handle: Option<&AppHandle>`，每个账户刷新后
  调用 `notify_account_events`（评估 → 入队 → emit，失败仅 warn 日志）；偏好存 settings 表
  （`notification_preferences` key），缺失/损坏回退默认值。
- **`app/mod.rs`**：注册 `tauri_plugin_notification::init()`；后台刷新循环与 `accounts_create`/
  `accounts_refresh` 命令传入 `AppHandle`；invoke_handler 注册 4 个新命令。
- **前端**：`types/`（通知视图模型/偏好/桥接 4 方法）、`lib/tauri.ts`（语义命令）、
  `lib/mock.ts`（演示通知 + 偏好）、`styles/copy.ts`（通知文案 + a11y）、新组件
  `features/dashboard/components/NotificationSection.tsx`（通知中心 + `<details>` 设置表单：
  开关立即提交、数字字段失焦提交、列表与偏好独立降级）、`App.tsx` 集成 +
  `notification:enqueue` 监听、`App.css` 通知样式（明暗主题、severity chip、窄窗换行）。

未改动：Tauri capabilities、CSP、网络层、凭据层、provider 适配器。

### 行为变化

- 后台刷新循环（1800s）与手动刷新后：每个账户触发一次通知评估；命中条件且通过 24h 去重的
  条目写入历史、弹出系统横幅（受静默/只提醒紧急过滤）并向前端发 `notification:enqueue`。
- 老库（v1）首次打开自动升级到 v2（生成 `*.pre-migration.bak` 备份后建表），既有数据不动。
- 仪表盘新增"通知"区块：未读徽标、逐条/全部已读、严重级别标签（文字 + 色彩双编码）、
  可折叠通知设置。

### 测试与构建（全部通过）

- Rust：`cargo test` **149 通过**（142 单元 + 7 fixture；新增通知评估 14、入队 4、存储 4）；
  `cargo fmt --all -- --check` 通过；`cargo clippy --all-targets --all-features -- -D warnings`
  0 错误 0 警告。
- 前端：`pnpm typecheck` 通过；`pnpm test` **30 通过**（新增 NotificationSection 8 个：渲染、
  未读徽标、逐条/全部已读、偏好即时提交、数字失焦提交、偏好失败降级、保存失败告警）；
  `pnpm build` 成功。
- 手动 UX（mock 桥接 + 浏览器，1280/375 双宽、暗色）：通知中心渲染与严重级别 chip、全部已读
  联动（徽标/按钮消失、条目保留变暗）、设置开关联动（总开关关闭禁用子控件 + `已保存` 反馈）、
  窄窗时间戳独立成行，均通过截图核对。

### Diff 自审（docs/07 §5）

1. **术语一致**：通知 kind/severity 命名与 docs/04 事件词汇、PRD §12 条件一一对应。
2. **失败不变量**：通知评估/派发失败仅 `tracing::warn`，绝不影响刷新结果；OS 派发失败静默。
3. **unknown ≠ zero**：仅 `Available`/`Partial` 快照参与阈值判定；无值不触发。
4. **状态可见性**：severity 用文字标签 + 颜色双编码；未读用圆点 + 徽标 + 已读按钮三重表达。
5. **IPC 边界**：前端只发语义命令；通知文本由 Rust 渲染并存储，webview 不拼装业务文案。
6. **权限边界**：capabilities 零改动（用户确认的方案）；OS 通知仅 Rust 端可触发。
7. **性能**：评估为每次刷新每账户一次纯函数；历史有保留上限；事件只在有新通知时发出。
8. **schema 变化**：v2 迁移新增 `notifications` 表，走备份 + 完整性检查 + 事务升级路径。
9. **secret 风险**：无——通知文本只含账户名与归一化数值；日志只记 kind/severity，不记消息体。
10. **架构耦合**：存储层不反向依赖 notifications 模块（`NotificationRecord` 自有类型），
    转换由 notifications 层负责；OS Sink 独立于评估逻辑，可替换可测试。

### 已知限制与下一阶段

- "OAuth/API key 即将过期"（PRD §12）仅在凭据已被明确拒绝时可判；主动预测过期时间需
  provider 侧元数据，未实现。
- Windows 通知横幅的实机弹出（安装版 AUMID 身份）未验证——开发环境无法覆盖，需安装版冒烟。
  已核实（tauri-plugin-notification 2.4.0 源码）：桌面端 `request_permission`/`permission_state`
  是恒 `Granted` 的桩（真实授权仅在移动端实现）；macOS 底层走遗留 `NSUserNotification` API，
  首次派发由系统自动弹出授权提示，无需显式申请。派发路径中的授权守卫因此是前向兼容设计：
  插件未来实现桌面授权时无需改代码；本机（Windows）工具链可完整编译验证该路径。
- 低额度/余额阈值变更不回溯评估（下次刷新生效）；通知不支持按账户静音。
- 下一阶段：历史趋势与基础图表（需 v3 迁移）→ 真实 provider 端点接入。

## Round 5 — 历史趋势：GitHub 风格热力图（每日 Tokens / 计费）（P1）

日期：2026-10-11 | 基线：Round 4 工作区 | 平台：Windows 11 x64

### 目标与依据

`IMPLEMENTATION-GAP.md` 中下一项 P1：**历史趋势与图表未实现**。依据 docs/01 §13（MVP：最近
7/30 天 usage、每个 quota window 的变化）、docs/02 §7（SQLite 保存 Usage points）。用户附加
要求：GitHub 风格热力图展示每一天的总 Tokens 使用量与计费额度，并用 better-* 设计 skills
（better-typography / better-colors / better-layout / better-ui）打磨整体 UI/UX。

### 需求完成矩阵

| 要求 | 来源 | 结果 |
|---|---|---|
| 历史数据持久化 | 02 §7 | ✅ 迁移 v3 `history_points` 表：快照键为指标身份、与快照写入同事务幂等采集、`observed_at` 索引、随账户级联删除、90 天保留（每次采集修剪） |
| 最近 7/30 天 usage | 01 §13 | ✅ 近 30 天统计卡（Tokens 总量 / 计费总额 / 活跃天数）+ 26 周热力图逐日数值 |
| 每日总 Tokens 使用量 | 用户要求 | ✅ token 指标 `used` 正增量求和；窗口重置造成的下降不计、重置前缺口不虚构 |
| 每日计费额度 | 用户要求 | ✅ currency 指标 `used` 增量 + balance 型指标 `remaining` 降幅（充值不算消费）；混币种日取主导币种并在文档注明 |
| GitHub 风格热力图 | 用户要求 | ✅ 26 周日历网格（月份/工作日标尺）、品牌色单色相 5 级强度阶梯（非零值四分位分档）、无数据日与零消耗日视觉区分、图例 少→多 |
| UI/UX 打磨 | 用户要求 | ✅ better-* 四项 skill 审查后实现：tabular-nums 统计、单色相语义（品牌青=用量，与额度条一致）、间距分组节奏、热力图容器内滚动（窄窗页面零溢出）、≤150ms 状态过渡、reduced-motion 降级 |
| 可达性 | 05 §13 | ✅ 方向键漫游焦点（WAI grid 模式，避免 182 个 tab 停留点）、逐日 sr-only 文案 + title 提示（日期 + Tokens + 金额）、图例非颜色单一编码、`:focus-visible` 环 |
| "不打扰"数据原则 | 总规范 | ✅ 未观测 ≠ 零：无记录日期渲染为空格；数值全部来自可信快照 |

### 代码变更

- **`storage/migrations/0003_history_points.sql` + `MIGRATIONS` 追加**：历史观测表（复合主键
  account/metric/observed_at，外键级联），schema v3。
- **`storage/mod.rs`**：`HistorySample` + `HISTORY_RETENTION_SECS`；`upsert_snapshot_set` 在同一
  事务内为可信快照（Available/Partial）写 `INSERT OR IGNORE` 历史行并按保留窗修剪；
  `history_samples(from, to)` 按 account/metric/时间排序返回。
- **`domain/history.rs`**：`DailySample`/`DailyUsage` + 纯函数 `aggregate_daily_usage(samples,
  local_offset)`——按 (account, metric) 分组取相邻正增量，token→Tokens、currency→费用、
  balance 降幅→费用、其他单位仅标记 has_data；`local_offset(timestamp)` 注入时区（生产用
  chrono Local，测试用固定偏移）；8 个新测试。
- **`commands/mod.rs`**：`history_summary` 命令（范围 7–365 天，默认 182）+ `HistoryDayViewModel`
  /`HistorySummaryViewModel` + `local_offset_at`；注册进 invoke_handler。
- **前端**：`types`（历史视图模型 + 桥接方法）、`lib/tauri.ts`、`lib/mock.ts`（确定性伪随机
  演示数据：工作日/周末节奏 + 采用爬坡 + 40 天无记录期）、`lib/format.ts`（`formatCompact`
  万/亿紧凑数值）、`styles/copy.ts`（历史文案 + a11y）、新组件
  `features/dashboard/components/HistorySection.tsx`（统计卡 + 热力图 + 图例 + 漫游键盘导航）、
  `App.tsx` 集成（`quota:updated` 事件驱动刷新）、`App.css`（热力图样式 + sr-only + 窄窗/减少动效）。
- **测试**：`src/test/HistorySection.test.tsx` 4 个（统计卡紧凑数值、逐日无障碍标签与图例、
  空态、ArrowLeft 漫游焦点）。

未改动：Tauri capabilities、CSP、网络层、凭据层、通知管线。

### 行为变化

- 每次成功刷新（后台 1800s 或手动）把可信快照写入历史；老库（v2）首次打开自动升级 v3
  （备份 + 完整性检查路径不变）。
- 仪表盘新增"历史趋势"区块：26 周热力图 + 近 30 天统计；`quota:updated` 事件同步刷新。
- 历史从本版本开始积累；安装前的日期显示为无数据（空格），不补零。

### 测试与构建（全部通过）

- Rust：`cargo test` **159 通过**（152 单元 + 7 fixture；新增聚合 8、采集 2）；fmt、clippy
  `-D warnings` 全绿。
- 前端：`pnpm typecheck`、`pnpm test`（**34 通过**，新增 4）、`pnpm build` 全部通过。
- 手动 UX（mock 桥接 + 浏览器截图，1280/375、暗色）：热力图与标尺渲染、强度阶梯与无数据
  区分、ArrowLeft 漫游焦点落点及单元格提示内容（"10月4日：5.89 万 Tokens，¥0.22"）、窄窗
  热力图容器内滚动且页面无横向溢出（scrollWidth == clientWidth）均通过。
- 明色主题未截图验证（强度阶梯以 alpha 混合构建、双主题共用令牌，理论自适应）；标注
  Not verified。

### Diff 自审（docs/07 §5）

1. **术语一致**：DailyUsage/HistorySample 与 docs/02 §7 "Usage points" 对应；热力图文案与
   copy.ts 单源。
2. **失败不变量**：历史采集随快照事务，失败即整体回滚，不影响快照正确性；`history_summary`
   失败仅令区块保持原状（前端 catch 保留旧数据）。
3. **unknown ≠ zero**：无观测日 `has_data=false` 渲染空格；错误/不可信快照不入历史；
   重置缺口不虚构。
4. **状态可见性**：强度阶梯 + 图例 + sr-only 文案三重编码；焦点环独立于颜色。
5. **IPC 边界**：聚合在 Rust 完成，前端只接收逐日视图模型；原始 history_points 不过 IPC。
6. **权限边界**：零改动。
7. **性能**：采集为同事务 1 条 INSERT/快照 + 1 条修剪；聚合单次 SQL 范围查询 + O(n) 遍历；
   热力图 182 个静态按钮 + 事件委托式键盘导航。
8. **schema 变化**：v3 迁移，走备份 + 完整性检查 + 事务升级。
9. **secret 风险**：无——历史只含归一化数值与单位；无凭据、无原始响应。
10. **架构耦合**：聚合时区以闭包注入（领域不依赖 chrono Local）；存储层 `HistorySample`
    为自有类型，与领域 `DailySample` 在命令层转换。

### 已知限制与下一阶段

- "每个 quota window 的变化"（PRD §13 MVP 剩余项）未做——需要按指标序列的窗口对比视图。
- 混币种日在热力图只显示主导币种金额；按币种拆分视图未做。
- 单位未知（`unit="unknown"`）的快照只标记 has_data，不计入 Tokens/费用（不猜单位）。
- 下一阶段：接入第一个真实 provider 官方限额端点 → quota window 趋势线。

## Round 6 — 桌面视图导航 + 供应商导轨 + 气泡双环与贴边吸附（P1）

日期：2026-10-11 | 基线：Round 5 工作区 | 平台：Windows 11 x64

### 目标与依据

用户需求（参照三张 macOS 菜单栏监控截图）：顶部供应商圆形图标导轨（仅显示已配置连通者，
数量随布局受限，末位为"全部面板"入口）；供应商专属视图带 2h/每日/每月用量切换；底部固定
功能区（设置/主窗口/GitHub 主页）替代整页纵向滚动；气泡收拢为当前供应商圆形图标 +
5h/7d 双活动圆环（Apple Watch 风格）；气泡支持四边贴边吸附持久化。设计与可达性按
better-typography / better-colors / better-layout / better-ui / better-accessibility 执行。

### 需求完成矩阵

| 要求 | 结果 |
|---|---|
| 顶部供应商圆形图标排列 | ✅ 导轨显示每个已配置账户的圆形品牌图标 + 状态点；`RAIL_MAX = 5`，超出显示 "+N" 并提示进概览面板；末尾固定"全部供应商"按钮 |
| 仅已配置连通者出现 | ✅ 导轨数据源即账户列表（已配置账户）；每个图标 aria-label 携带实时健康状态 |
| 点击图标 → 供应商专属视图 | ✅ 滑入该账户的额度卡片 + 用量统计卡 + 刷新按钮；导轨图标 accent 高亮 |
| 2h / 每日(24h) / 每月(30d) 统计切换 | ✅ 分段控件三档切换；数据来自新命令 `usage_windows`（`range_usage` 正增量推导：token→Tokens、currency used 增量/balance 降幅→计费）；窗口无记录显示"暂无记录"而非零 |
| 底部固定功能区 | ✅ Dock：概览/历史/通知/设置（layoutId 药丸高亮、aria-pressed）+ GitHub 项目主页（`open_project_page` 命令，URL 编译期常量，opener Rust API，webview 无任意外链能力） |
| 告别整页纵向滚动 | ✅ 应用外壳：标题栏 + 导轨 + 可滚动视图区 + 固定 Dock；视图间方向感知滑入滑出（reduced-motion 淡入淡出）；设置视图收纳表单/检查更新/本地声明 |
| 气泡双活动圆环 | ✅ 主账户窗口化额度按时长升序取前二映射双环（4–6h→"5h"、144–192h→"7d"、其余实际跨度），外环短窗内环长窗、中心品牌图标；无窗口额度回退单环压力；无可信数据回退猫头鹰图标 |
| 四边贴边吸附持久化 | ✅ `persist_bubble_position` 内 48px 阈值逐轴吸附（角落双轴）、吸附后窗口回弹到目标位置并连同 SQLite 持久化；`show_bubble` 恢复吸附位 |

### 代码变更

- **`domain/history.rs`**：`RangeUsage` + `range_usage(samples, from, to)`（正增量窗口求和，
  基线取窗口前最近一次观测；重置/充值规则与日聚合一致）+ 2 个测试。
- **`commands/mod.rs`**：`usage_windows` 命令（2h/24h/30d 三档，可按账户过滤）；
  `open_project_page` 命令（常量 URL，OpenerExt Rust API）。
- **`app/mod.rs`**：`persist_bubble_position` 增加 `snap_to_edge`（逐轴 48px 阈值，吸附后
  `set_position` 回弹）+ 注册两个新命令。
- **前端**：`types`（RangeUsage/UsageWindows + 桥接 `usageWindows`）、`shell.openProjectPage`、
  `mock.ts` 演示用量、`copy.ts`（views/rail/usage/bubble 文案与 a11y 标签）、
  `components/QuotaMetric.tsx`（自 App 抽出复用）、`components/ProviderView.tsx`（新）、
  `App.tsx` 重构为应用外壳（导轨 + AnimatePresence mode="wait" 视图切换 + Dock + 设置视图，
  移除顶部"+"入口与页脚，改由 Dock/设置视图承载）、`BubbleView.tsx` 双环重写、
  `icons.tsx` 新增 IconGear/IconExternal/IconGrid、`App.css`（导轨/Dock/pview/双环/窄窗规则）。
- **测试**：既有 34 个 vitest 与全部 cargo 测试保持通过（桥接 mock 补 `usageWindows`）。

未改动：Tauri capabilities、CSP、网络层、凭据层、通知管线、迁移 schema（无 v4）。

### 测试与构建（全部通过）

- Rust：`cargo test` **161 通过**（154 单元 + 7 fixture）；fmt、clippy `-D warnings` 全绿。
- 前端：`pnpm typecheck`、`pnpm test`（34）、`pnpm build` 全部通过。
- 手动 UX（mock 桥接 + 浏览器截图，1280/375、暗色）：概览 + 导轨 + Dock 布局、供应商视图
  tabs 切换数值联动（24h→141.89 万 / 2h→10.29 万）、设置视图、Dock 标签可见性（DOM 实测
  5 个标签可见）、窄窗（375）Dock 收纳图标 + 页面零横向溢出。
- Not verified：明色主题截图；气泡双环与贴边吸附的实机窗口行为（Tauri 窗口行为无法在
  纯浏览器预览覆盖，吸附逻辑在 Rust 侧且 clippy/test 通过）。

### Diff 自审（docs/07 §5）

1. **术语一致**：views/rail/usage 文案入 copy.ts 单源；Dock/导轨 aria 标签齐全。
2. **失败不变量**：`usage_windows` 失败时供应商视图保留额度卡片（统计卡显示"暂无记录"）；
   视图切换不重置账户数据。
3. **unknown ≠ zero**：窗口无观测 → `has_data: false` →"暂无记录"；双环无窗口额度 → 回退
   而非画零环。
4. **状态可见性**：导轨状态点 + aria 文字 + title 三重表达；Dock 药丸高亮 + aria-pressed。
5. **IPC 边界**：外链 URL 为 Rust 常量，webview 不可指定任意外链；用量聚合在 Rust 完成。
6. **权限边界**：零改动（opener 已有权限，Rust 端调用不经能力门）。
7. **性能**：视图切换挂载/卸载对应区块（AnimatePresence mode="wait"），无隐藏页常驻；
   usage_windows 单次范围查询。
8. **schema 变化**：无。
9. **secret 风险**：无。
10. **架构耦合**：QuotaMetric 抽为共享组件消除 App 内联重复；范围聚合与日聚合共享规则但
    独立实现，避免为复用引入过度抽象。

### 已知限制与下一阶段

- 供应商导轨图标来自 `providerId` → lobe 图标映射；generic.openai_compatible 显示通用钥匙
  图标，接入真实 provider 后自动切换为品牌图标。
- 2h/24h 统计在安装初期历史不足时诚实显示"暂无记录"。
- 气泡双环的窗口标注依赖 provider 上报 startsAt/endsAt；仅有 reset_at 的端点回退单环。
- 下一阶段：接入第一个真实 provider 官方限额端点 → quota window 趋势线 → 实机验证
  双环/吸附/通知横幅。

## Round 7 — 品牌图标映射 + recharts 趋势图 + 视图内容净化与热力图纵向填充

日期：2026-10-11 | 基线：Round 6 工作区 | 平台：Windows 11 x64

### 需求完成矩阵

| 要求 | 结果 |
|---|---|
| 按 lobehub skill 使用 @lobehub/icons，真实品牌图标 | ✅ `ProviderIcon` 重写为 providerId **冒号前缀**映射（30 个品牌前缀 → lobe 图标）；`generic.openai_compatible` 等未知前缀回退中性钥匙图标；mock 演示改用 openai/anthropic/deepseek/gemini 四品牌，导轨/卡片/供应商视图/Dock 全部显示真实品牌图标 |
| Dock 各页只显示当前页内容 | ✅ 汇总统计卡（额度压力/账户/正常/下次重置）从全局区移入概览视图；历史/通知/设置/供应商视图不再出现仪表盘元素；全局仅保留瞬时通知与更新横幅 |
| 布局适应完整页面空间；热力图铺满纵向 | ✅ 历史视图弹性布局：趋势卡固定高度 + 热力图 `flex: 1` 铺满剩余纵向（单元格 1fr 流式伸缩、`min-width: max-content` 窄窗横向滚动）；修复级别色被后声明的基础规则覆盖导致的"全灰"缺陷 |
| GitHub 跳转显示站点图标 | ✅ 新增填充式 `IconGitHub`（章鱼猫 mark），Dock 入口改为纯图标（aria-label 保留），设置视图同步替换 |
| 第三方图表库优化趋势/用量可视化 | ✅ 引入 **recharts 3.10**（纯本地渲染，无外部请求，CSP 不变）：历史视图近 30 天用量趋势面积图（品牌青单色相、`var(--brand)` 直入 SVG、断线不补零、峰值标注、深色适配 Tooltip）；供应商视图 30 天迷你趋势（`history_summary` 新增 `account_id` 过滤参数） |

### 代码变更与验证

- `components/ProviderIcon.tsx`（前缀映射重写）、`components/icons.tsx`（+IconGitHub）、
  `commands/mod.rs`（history_summary 按账户过滤）、`lib/tauri.ts`（签名扩展）、`lib/mock.ts`
  （品牌多样化）、`App.tsx`（stats 移入概览 + Dock 图标化）、`HistorySection.tsx`
  （recharts 趋势 + 流式热力图）、`ProviderView.tsx`（迷你趋势）、`App.css`（弹性布局 +
  级别色级联修复 + sr-only 等沿用）、`package.json`（+recharts）。
- 验证：`cargo test` 161（154+7）、fmt、clippy `-D warnings`；vitest 34、typecheck、build 全绿。
- 浏览器审计（1280，暗色，mock）：导轨四品牌图标、趋势面积图完整渲染（峰值 34.17 万）、
  热力图纵向铺满 + 空数据期与强度阶梯区分、供应商视图迷你趋势、Dock GitHub 图标。
- Not verified：明色主题；实机双环/吸附（同前轮）。

## Round 8 — 字体/图标可见度审计 + 趋势悬停详情（供应商分解 + 额度概览）

日期：2026-10-11 | 基线：Round 7 工作区 | 平台：Windows 11 x64

### 需求完成矩阵

| 要求 | 结果 |
|---|---|
| 字体/字重/数字可见度 | ✅ 主要数值强调升级：额度卡"剩余"从 13px→17px（--step-4、650 字重）；历史统计卡与供应商用量数值统一 650；Dock 标签 500；全部数值保持 tabular-nums。字体栈维持系统栈（CSP 禁网络字体，Segoe UI Variable/PingFang 等为平台最佳选择） |
| 图标与圆边框贴合 + 尺寸 | ✅ `.provider-icon` 改 grid 居中 + `line-height: 0`；尺寸整体上调：导轨 22→25（圆钮 34→36）、账户卡 28→32、供应商视图头 34→40、气泡 18→20 |
| 趋势悬停详情 | ✅ 自定义 Tooltip：日期 + 当日 Tokens 总量 + **按供应商分解**（品牌图标 + 各自 tokens，新命令 `history_breakdown` 按日按账户聚合）+ **当前额度概览**（各账户窗口化额度百分比，`quotaWindowTag` 抽为共享 helper 并增加指标名称回退——无 startsAt/endsAt 时从供应商自己的指标命名推断 5h/7d/Nd/Nh，推断不出如实显示"未知"） |
| 模型型号说明 | ⚠️ 本机不采集模型级用量（历史只有账户×指标维度），分解到供应商/账户层级；模型维度需 provider 上报 per-model usage，记入下一阶段 |

### 代码变更与验证

- `lib/format.ts`（quotaWindowTag 共享 + 名称回退）、`commands/mod.rs`（`history_breakdown` +
  视图模型）、`app/mod.rs`（注册）、`types`/`lib/tauri.ts`/`lib/mock.ts`（桥接与演示数据）、
  `HistorySection.tsx`（TrendTooltip + accounts 属性 + 趋势点携带 dayStart）、
  `BubbleView.tsx`（改用共享 quotaWindowTag）、`App.css`（数值字重/图标居中/trend-tip 样式）、
  组件尺寸上调、测试桥补 `historyBreakdown` + recharts/ProviderIcon 桩（vitest 无法解析
  lobehub 传递 JSON 导入——生产构建不受影响，图表与图标由浏览器审计覆盖）。
- 验证：cargo 161（154+7）、fmt/clippy 全绿；vitest 34、typecheck、build 全绿。
- 浏览器审计（浅色主题）：Tooltip 显示"9月28日 · Tokens 3.83 万 + 四供应商分解 + OpenAI 官方
  5h 84% · 7d 90%"；明色主题首次截图验证通过（此前 Not verified 项消除）。

## Round 9 — 全局字体阶梯上调 + 图标细边框根除 + 状态点/图标可见度

日期：2026-10-11 | 基线：Round 8 工作区 | 平台：Windows 11 x64

### 需求完成矩阵

| 要求 | 结果 |
|---|---|
| 全部文案可见度 | ✅ 字号阶梯整体上调一档：--step-0/1/2 从 11/12/13px → 12/13/14px（辅助文字不再低于 12px，符合 better-typography 下限）；字重层级保持 400/500/600/650 分明；可视化数值（额度"剩余"、统计卡、用量卡）17px/650 + tabular-nums |
| 图标细边框根除 | ✅ 根因是 `.provider-icon` 的 `outline: 1px solid oklch(0 0 0 / 0.1)`（为居中外观加的发丝描边），彻底移除；回退图标的自带底板同步去掉，图标直接贴合面板 |
| 顶部导轨图标调大 | ✅ 圆钮 34→40px、图标 25→28px |
| 状态点更大更直观 | ✅ 9→12px，改用高饱和系统色（健康 #34c759 / 关注 #ff9500 / 紧急 #ff3b30），明暗主题都一眼可辨 |
| 其余图标统一调大 | ✅ Dock 17→19、标题栏气泡 15→17、账户/供应商刷新与删除 16→18、统计/通知/护盾 14→15、重置时钟 13→14 |
| gemini 图标不可见 | ✅ lobe 的 Gemini color 变体为浅色渐变，浅色主题不可见——gemini 前缀改映射 Google 四色 G（明暗主题均高对比）；mock 供应商显示名同步（Anthropic/DeepSeek/Google Gemini） |

### 验证

- vitest 34、typecheck、build、fmt、clippy 全绿（无 Rust 改动，cargo 结果不变）。
- 浏览器特写审计（浅色 1280）：导轨四品牌图标全部清晰、零描边、状态点高饱和直观。

## Round 10 — Provider 能力矩阵（docs/09）：manifest 元数据 + 目录面板 + 诚实等级标注

日期：2026-10-11 | 基线：Round 9 工作区 | 平台：Windows 11 x64

### 需求完成矩阵

| 要求 | 结果 |
|---|---|
| 能力矩阵元数据入配置 | ✅ `ProviderManifest` 新增 `capability` 段（level/accountTypes/quotaUnits/targetWindows/priority，serde default 兼容旧 YAML）；8 个内置 YAML 按 docs/09 首发行补齐 |
| 运行时目录 | ✅ 8 个 YAML 经 `include_str!` 嵌入运行时；`provider_matrix()` 输出 docs/09 全部 28 行目录——已实现者等级以 YAML capability 为权威（OpenAI/Anthropic/OpenRouter L4，DeepSeek/Qwen/Kimi/Gemini L3，MiniMax L2），未实现者按规划如实不标等级（docs/09 明示"首发规划，需 fixture 验证"） |
| 命令暴露 | ✅ `provider_matrix` 命令（行结构 serde camelCase 直接复用） |
| 前端展示 | ✅ 设置视图新增"供应商能力矩阵"面板：L4 青色/L3 蓝色分级徽章 + 规划行灰显"规划中 · Px" + 单位/窗口元数据 + 图例说明；供应商视图头部显示 L1 · 自定义接入徽章 |
| 诚实展示规则 | ✅ 规划行不发明等级；generic 兼容账户按 docs/09 定义标 L1 自定义接入；"未知不补零"规则不受影响 |

### 代码变更与验证

- `providers/manifest/mod.rs`（ManifestCapability + level_id + 测试扩展）、8 个内置 YAML
  （capability 块）、`providers/builtin/mod.rs`（BUILTIN_MANIFESTS 嵌入 + ProviderMatrixRow +
  provider_matrix() + 2 个一致性测试：目录 28 行/8 已实现等级、YAML 与目录逐项匹配）、
  `commands/mod.rs` + `app/mod.rs`（provider_matrix 命令注册）、前端 ProviderMatrix 组件/
  ProviderView L1 徽章/copy/types/bridge/mock/CSS（cap-chip 体系）。
- 验证：`cargo test` 163（156 单元 + 7 fixture，新增矩阵一致性 2 个）、fmt/clippy 全绿；
  vitest 34、typecheck、build 全绿；浏览器审计（暗色）：矩阵面板 28 行分级渲染、规划行
  灰显、供应商视图 L1 徽章。

## Round 11 — 能力矩阵驱动供应商配置（manifest 接入档案 + 端点门控）

日期：2026-10-12 | 基线：Round 10 工作区 | 平台：Windows 11 x64

### 需求完成矩阵

| 要求 | 结果 |
|---|---|
| 矩阵驱动配置 | ✅ `manifest_profiles` 命令输出 8 个内置供应商接入档案（baseUrl/能力等级/声明端点，直出嵌入 manifest）；设置表单新增供应商选择器——选中即按清单自动填充接口地址（只读）并隐藏手填端点，改显"内置端点"徽章（如 DeepSeek：模型/余额） |
| 端点按能力门控 | ✅ 后端 `resolve_new_account`：内置供应商的 baseUrl 与端点以 manifest 为权威，客户端路径被忽略；仅清单声明的端点可被探测（DeepSeek 只有 balance，quota/usage 为 None） |
| 真实品牌账户 | ✅ 内置供应商账户的 provider id 为 `deepseek:account-x` 等真实前缀——导轨/卡片/气泡自动显示品牌图标与显示名，不再是通用兼容外观 |
| **既有缺陷修复** | ⚠️ 发现并修复：通用账户创建路径把 `api_key` 当作 `base_url` 传入 `OpenAiCompatibleConfig::new`，而表单的 baseUrl 从未持久化——validate 的 http(s) 检查使**界面创建通用账户必然失败**（凭据在写入前即返回，无泄漏）。修正为传 `resolved.base_url` 并在解析层加 http(s) 前置校验 |

### 代码变更与验证

- `providers/builtin/mod.rs`（ManifestProfile + manifest_profiles）、`commands/mod.rs`
  （manifest_profiles 命令 + resolve_new_account 重构 + persist 使用 ResolvedNewAccount +
  3 个新测试：内置档案解析/未知品牌拒绝/非 http baseUrl 拒绝）、前端 SetupForm 重写
  （供应商选择器 + 能力门控表单）、types/bridge/mock/copy/CSS（picker 体系）。
- 验证：`cargo test` 166（159 单元 + 7 fixture）、fmt/clippy 全绿；vitest 34、typecheck、
  build 全绿；浏览器审计（暗色）：选择器六品牌渲染、DeepSeek 选中后 baseUrl 只读预填 +
  端点输入隐藏 + 内置端点徽章、自定义模式保持原有三端点表单。

## Round 12 — 供应商生态扩容（21 个内置）+ 协议分类 + 密钥输入加固

日期：2026-10-12 | 基线：Round 11 工作区 | 平台：Windows 11 x64

### 需求完成矩阵

| 要求 | 结果 |
|---|---|
| 新增更多供应商配置 | ✅ 内置清单 8→21：新增 Groq/Mistral/xAI/SiliconFlow/StepFun/Zhipu/Doubao/Baichuan/01.AI/360（均为 OpenAI 兼容协议，L2；SiliconFlow L3 含余额端点）+ 网关 New API/One API/LiteLLM（`customBaseUrl: true`，baseUrl 由用户填入）。目录仍 28 行，规划行只剩 7 个非 OpenAI 协议/未定协议供应商 |
| API 协议顶层分类 | ✅ manifest 新增 `apiFormat`（默认 openai-compatible）；Anthropic 标 `anthropic`、Gemini 标 `google-ai`；选择器分组显示——OpenAI 兼容协议可选，其他协议按钮禁用并提示"适配规划中"（避免用 OpenAI 解析去打原生协议）；矩阵行携带协议字段 |
| Kimi 图标修复 | ✅ 根因：lobe 的 provider 枚举无 `kimi` id（仅 `moonshot`/`kimicodingplan`），generic ProviderIcon 解析不到即渲染空。`kimi` 前缀改映射 `moonshot`，Moonshot 标志正常显示 |
| API 密钥框防自动填充 | ✅ 密钥输入改为 `type="text"` + `-webkit-text-security: disc` 圆点遮蔽 + `autocomplete="off"`——浏览器不再视为密码字段，密码生成/自动填充提示不再出现（Tauri 的 Chromium/WebKit 均支持该属性） |

### 代码变更与验证

- manifest schema（apiFormat/customBaseUrl）、13 个新 YAML、anthropic/gemini 协议标注、
  `providers/builtin/mod.rs`（BUILTIN_MANIFESTS 21 项、注册表改 manifest 驱动并移除硬编码
  BUILTIN_PROVIDERS/builtin_adapter、registry 新增 all()、profiles/matrix 携带协议与网关标记、
  计划表剪枝至 7 行）、前端 ProviderIcon 映射扩展与 kimi 修复、SetupForm 协议分组选择器 +
  网关可编辑地址 + secret-input 加固、types/bridge/mock/copy、registry all()。
- 验证：`cargo test` 166、fmt/clippy 全绿（含"registry 含 21 适配器"新断言）；vitest 34、
  typecheck、build 全绿；浏览器审计（暗色）：选择器九个品牌按钮 + Kimi/Moonshot 图标正常、
  New API 网关 baseUrl 占位"网关地址"且可编辑、内置端点徽章仅"模型"、密钥框 type=text。
