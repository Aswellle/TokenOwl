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
