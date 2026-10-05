# TokenOwl 工程决策

## 2026-10-05：从官方 Tauri 2 React TypeScript 模板初始化

- **Why**：原始目录只有正式文档，没有源码或远程提交可供同步。
- **Scope**：建立 Vite、React、TypeScript、Tauri 2、Rust library/desktop entry 的可编译骨架。
- **Risk**：模板默认依赖和窗口配置可能与最终产品不同。
- **Migration**：保留标准 `src-tauri/src/lib.rs` 作为 Tauri/mobile 共用入口；领域模块通过 Rust module 隔离。
- **Rollback**：删除初始化源码和配置不会触碰正式文档；Git 首个 checkpoint 前不执行破坏性清理。
- **Validation**：必须运行 npm build、cargo test、cargo clippy、cargo fmt 和 Tauri build。

## 2026-10-05：统一数据契约优先于 UI

- **Decision**：所有 quota、usage、balance 和 reset 都以 Rust domain model 的 `Option` 字段表达，缺失值保持 `null`。
- **Why**：正式规范要求 unknown != zero，且 Provider capability 由真实接口和 fixture 决定。
- **Impact**：React 只消费语义 ViewModel；不能从缺失字段推导伪造百分比或倒计时。

## 2026-10-05：凭据与普通数据分离

- **Decision**：账户只保存 `credential_ref`，后续由 Rust credential service 对接 Windows Credential Manager/macOS Keychain。
- **Why**：API Key、OAuth token、Cookie 和 session 不得进入 SQLite 或日志。
- **Impact**：Provider adapter 接收解析后的 credential context，不直接读取 UI 或数据库 secret。

## 2026-10-05：后台刷新与 UI 生命周期分离

- **Decision**：scheduler、snapshot store 和 UI event subscription 采用独立边界；Dashboard 关闭不停止后台状态。
- **Why**：正式架构要求 Bubble、Tray 和后台刷新持续工作。
- **Impact**：第一阶段先实现可测试的刷新状态接口，再接入真实网络和 SQLite。
