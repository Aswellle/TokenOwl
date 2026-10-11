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
