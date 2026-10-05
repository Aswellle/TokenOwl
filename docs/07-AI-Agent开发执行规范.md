# TokenOwl AI Agent 开发执行规范

> 本文件是交给 AI 编程 Agent 的执行约束。优先级高于“顺手优化”“自由重构”“额外功能”。

## 0. 总执行规则

你不是在一次性生成一个 Demo，而是在维护一个需要长期发布、升级、跨平台运行的桌面软件。

**任何改动都必须保证：现有功能不被无意删除；每个阶段完成后项目仍然可以编译、运行、测试；不得为了实现新功能而破坏已有 Provider、UI、数据或发布链路。**

---

## 1. 执行前检查

开始任何代码修改前，必须：

1. 阅读 `README.md` 和本目录全部设计文档。
2. 检查当前 git status。
3. 获取当前分支和 commit。
4. 建立可回滚 checkpoint。
5. 安装并锁定依赖。
6. 运行 baseline tests。
7. 运行 baseline build。
8. 记录当前已知失败项。

输出：

```text
BASELINE.md
- commit
- branch
- platform
- build status
- test status
- known failures
```

---

## 2. 禁止行为

禁止：

- 无审查地重写整个项目；
- 删除现有功能以“简化架构”；
- 修改 lockfile 后不验证；
- 把 secret 写入代码、fixture 或日志；
- 为了 CI 通过而删除测试；
- 为了 UI 好看而破坏可访问性；
- 将 Provider API hard-code 到 React component；
- 使用真实账户数据作为唯一测试依据；
- 没有 rollback 就修改 SQLite migration；
- 未测试 Windows/macOS 就声称跨平台完成；
- 没有实际构建产物就声称“release ready”。

---

## 3. 分阶段执行

### Phase 0 — Baseline

目标：

- 项目能启动；
- 确定 Tauri 2 / React / Rust 基线；
- CI 可执行；
- lint/test/build 基线完成。

DoD：

```text
pnpm lint
pnpm test
pnpm build
cargo fmt --check
cargo clippy -- -D warnings
cargo test
```

---

### Phase 1 — Shell / Bubble

完成：

- tray；
- bubble window；
- drag；
- position persistence；
- dashboard shell；
- theme；
- keyboard access。

必须先生成截图/录屏验证交互。

---

### Phase 2 — Domain / Storage / Credential

完成：

- Provider domain；
- Account domain；
- QuotaSnapshot；
- SQLite；
- migration；
- Keychain；
- Scheduler skeleton。

不得在 UI 中临时造 quota calculation。

---

### Phase 3 — Provider Protocol

完成：

- trait；
- registry；
- manifest schema；
- fixture test harness；
- custom OpenAI-compatible provider。

先做 Generic Adapter 再批量做厂商 Adapter。

---

### Phase 4 — Provider Expansion

每增加一个 Provider，必须单独提交：

```text
provider implementation
fixture files
tests
docs/provider capability matrix
```

---

### Phase 5 — Reset / History

完成：

- 5h；
- 7d；
- daily；
- monthly；
- rolling；
- balance；
- bonus；
- history；
- notifications。

---

### Phase 6 — Polish

审查：

- UI spacing；
- typography；
- motion；
- hover/pressed/focus；
- stale state；
- empty state；
- error state；
- high-DPI；
- multiple displays；
- Windows/macOS 差异。

---

### Phase 7 — Release

必须实际产出：

- Windows installer；
- macOS installer/app bundle；
- checksum；
- release notes；
- update metadata。

---

## 4. 每项任务的标准工作流

```text
Read
 ↓
Inspect
 ↓
Baseline
 ↓
Design change
 ↓
Implement
 ↓
Unit test
 ↓
Integration test
 ↓
Build
 ↓
Manual UX test
 ↓
Diff review
 ↓
Self-audit
 ↓
Commit
```

任何一步失败都不得宣称任务完成。

---

## 5. 修改前后的 Diff Audit

完成后必须回答：

```text
1. 修改了哪些文件？
2. 每个文件为什么修改？
3. 删除了什么？为什么？
4. 有没有行为变化？
5. 哪些文档要求尚未实现？
6. 哪些功能只能部分支持？
7. Windows/macOS 是否都验证？
8. 是否改变了数据 schema？
9. 是否需要 migration？
10. 是否新增 secret handling 风险？
```

---

## 6. Provider 实现自检

每个 Provider 必须输出：

```text
Provider ID
Auth method
Endpoint(s)
Quota capability
Balance capability
Usage capability
Reset capability
Data source
Freshness policy
Rate-limit handling
Fixture coverage
Known limitations
```

如果供应商没有公开 quota API：

```text
不要伪造
不要硬编码假值
不要把 model endpoint response 当成 quota
必须标记 capability unavailable
```

---

## 7. UI 自检

每个界面都要检查：

```text
Loading
Empty
Success
Partial
Stale
Error
Disabled
Offline
Long provider name
Long account name
Large number
Unknown unit
Missing resetAt
Narrow window
High DPI
Dark/light
Reduced motion
Keyboard
```

---

## 8. 每阶段必须可发布

阶段结束必须至少满足：

```text
Compile ✅
Unit tests ✅
Integration tests ✅
No new warnings of significance ✅
App launches ✅
Existing primary flows ✅
```

允许尚未完成的功能通过 feature flag 隐藏。

不允许通过“留一个 TODO 后面再修”把半成品暴露给普通用户。

---

## 9. Agent 自主差距审计提示词

把下面内容作为每轮开发后的审计指令：

```text
你现在进入 TokenOwl Implementation Audit 阶段。

不要假设自己完成了本轮任务。你必须以仓库当前代码为唯一事实依据，逐条对照 docs/ 下的产品、架构、Provider、UI/UX、数据、安全、测试、发布文档。

输出：
1. 已完全实现的要求；
2. 部分实现的要求；
3. 尚未实现的要求；
4. 行为与文档不一致的地方；
5. 原有功能是否被破坏；
6. 是否出现新的架构耦合；
7. 是否存在 Provider hard-code 泄漏到 UI；
8. 是否存在 secret 明文、日志泄漏或 unsafe storage；
9. 是否存在 UI loading/error/stale/empty state 缺失；
10. Windows/macOS 构建与行为差异；
11. 测试覆盖缺口；
12. 发布风险。

对每一个结论给出：
- 文件路径；
- 代码位置；
- 当前行为；
- 文档要求；
- 差距；
- 最小修复动作；
- 风险级别 P0/P1/P2。

不得为了得到“全部完成”结论而弱化验收标准。
```

---

## 10. P-Level

### P0

- 数据错误导致用户做错误额度判断；
- secret 泄露；
- app 无法启动；
- provider crash 导致整个 scheduler 崩溃；
- migration 损坏用户数据库；
- release 安装失败。

### P1

- 单 Provider 无法工作；
- 重要 reset 计算错误；
- stale 数据没有提示；
- Windows/macOS 主流程行为不一致；
- UI 严重阻塞。

### P2

- 动效细节；
- tooltip；
- 非核心 provider；
- 高级图表；
- polish。

---

## 11. Commit 原则

推荐：

```text
feat(shell): add floating bubble window
feat(domain): add quota window model
feat(provider): add qwen adapter
feat(provider): add custom openai compatible adapter
feat(history): add usage snapshots
fix(provider): preserve stale snapshot on 429
fix(ui): distinguish stale and disconnected states
```

每个 commit 尽可能单一职责。

---

## 12. 完成后输出

必须输出：

```text
IMPLEMENTATION_REPORT.md
```

包含：

- 需求完成矩阵；
- 代码变更；
- 测试；
- 构建；
- 截图/UX 验证；
- 已知缺陷；
- 下一阶段；
- 是否可以发布。

---
