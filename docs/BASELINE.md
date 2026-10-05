# TokenOwl 工程基线

记录日期：2026-10-05。此记录描述第一次修改前的状态，后续验证记录见 IMPLEMENTATION-STATUS.md。

| 项目 | 原始状态 |
|---|---|
| Git commit / branch | 本地不存在 .git，无 commit / branch；git status、branch、log 均返回 not a git repository |
| Remote | https://github.com/Aswellle/TokenOwl.git；git ls-remote HEAD / main / master / --heads 成功退出，无引用输出，未读取到远程提交 |
| OS | Windows x64 10.0.22631 |
| Node | v22.14.0 |
| npm | 10.33.2 |
| Rust / Cargo | rustc 1.94.0 / cargo 1.94.0 |
| Tauri | 本地 CLI 不存在；create-tauri-app 4.7.4 可执行 |
| 文件 | README.md + docs/ 中 12 份正式技术文档；没有源码、package.json、Cargo.toml、lockfile、测试或 CI |
| build | npm run build 退出 38：package.json ENOENT |
| test | npm test 退出 38：package.json ENOENT |
| lint | npm run lint 退出 38：package.json ENOENT |
| typecheck | 尚无脚本；初始化模板后 npm run typecheck 返回 Missing script，不算原始通过 |
| cargo test | 退出 101：Cargo.toml 不存在 |
| cargo clippy -- -D warnings | 退出 101：Cargo.toml 不存在 |
| cargo fmt --check | 退出 1：Cargo.toml 不存在 |

## 原有失败、风险与技术债

尚无可安装、运行、测试或打包的产品。全部 MVP 功能待实现。正式文档内部的 Phase 编号和优先级表存在差异，按用户提供的总控提示词优先级解释。原始文档不修改其验收标准。

## 初始化来源

使用官方 create-tauri-app 4.7.4 的 Tauri 2 React TypeScript 模板作为配置输入，保留正式文档。当前源码先在根目录采用标准 Tauri 布局，Rust lib 作为 UI 无关模块的入口；后续依据实际规模决定是否迁移 monorepo。没有覆盖远程分支或用户已有代码。
