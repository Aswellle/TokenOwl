<p align="center">
  <img src="apps/desktop/public/tokenowl-icon.png" alt="TokenOwl icon" width="120" />
</p>

<h1 align="center">TokenOwl</h1>

<p align="center">
  <strong>你的 AI 额度，一眼可见 —— 数据始终留在桌面，绝不上云。</strong><br/>
  一个本地优先的 AI 供应商额度、用量与余额监视器。
</p>

<p align="center">
  <a href="#功能特性">功能特性</a> ·
  <a href="#支持的供应商">支持的供应商</a> ·
  <a href="#安装">安装</a> ·
  <a href="#工作原理">架构</a> ·
  <a href="#参与开发">参与开发</a> ·
  <a href="#安全">安全</a>
</p>

<p align="center">
  <a href="https://github.com/Aswellle/TokenOwl/actions/workflows/ci-rust.yml"><img src="https://img.shields.io/github/actions/workflow/status/Aswellle/TokenOwl/ci-rust.yml?branch=main&label=rust%20CI&style=flat-square" alt="Rust CI" /></a>
  <a href="https://github.com/Aswellle/TokenOwl/actions/workflows/ci-frontend.yml"><img src="https://img.shields.io/github/actions/workflow/status/Aswellle/TokenOwl/ci-frontend.yml?branch=main&label=frontend%20CI&style=flat-square" alt="Frontend CI" /></a>
  <a href="https://github.com/Aswellle/TokenOwl/releases"><img src="https://img.shields.io/github/v/tag/Aswellle/TokenOwl?label=release&style=flat-square" alt="Release" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-green?style=flat-square" alt="MIT License" /></a>
  <img src="https://img.shields.io/badge/platform-Windows%20%7C%20macOS-blue?style=flat-square" alt="Platform" />
</p>

<p align="center">
  <strong>简体中文</strong> · <a href="README.en.md">English</a>
</p>

---

你可能同时管理着 OpenAI、Anthropic、Gemini、DeepSeek 的 API 密钥，外加一台自托管的 LiteLLM 网关，还有那个你总忘记查看余额的积分制国内供应商。然后某一次生成进行到一半，`429` 不期而至。

TokenOwl 是一个小小的桌面伴侣，它回答一个问题 —— 而且**总是赶在**意外发生之前回答：*我还剩多少额度？什么时候恢复？*

它以一枚置顶悬浮球的形式常驻屏幕一角，点击展开完整仪表盘，而这一切**完全运行在你的机器上** —— API 密钥进入系统凭据库，数据写入本地 SQLite 文件，没有任何数据外发。

<p align="center">
  <img src="assets/dashboard.png" alt="TokenOwl 仪表盘：额度压力汇总、各账户额度卡片与重置倒计时、供应商导轨" width="820" />
</p>

## 为什么不直接看各家网页控制台？

| | 供应商网页控制台 | TokenOwl |
| :--- | :--- | :--- |
| 查看一家供应商 | 打开网站、登录、层层导航 | 瞄一眼悬浮球 |
| 查看五家供应商 | 五个标签页、五次登录 | 一块仪表盘 |
| 工作中断额提醒 | 等下一次 `429` 才知道 | 通知**赶在**中断之前 |
| 用量趋势 | 每家各自为政 | 一张热力图覆盖所有账户 |
| 密钥存在哪里 | 浏览器会话 | 系统凭据库，绝不落盘 |
| 用量数据存在哪里 | 他们的服务器 | 你的 SQLite 文件 |

## 功能特性

**统一监控** —— 接入不同供应商的账户，在一个归一化的视图里查看剩余额度、用量、余额与重置时间。Token 数、金额、请求数、积分 —— 各按其原生单位呈现，绝不换算成臆造的数字。

**诚实的未知值** —— 供应商不报告上限时，TokenOwl 显示*未知*，而不是 `0 / 100`。每个数值都携带数据来源与新鲜度（`fresh` / `stale` / `expired`），陈旧数据绝不冒充实时的。

**悬浮球** —— Apple Watch 风格双环，置顶、可拖拽、屏幕边缘吸附。外环映射你最短的重置窗口（如 5 小时），内环映射长窗口（如每周）；中央是供应商品牌图标。点击即打开仪表盘。

<p align="center">
  <img src="assets/bubble-rings.png" alt="双额度环悬浮球" width="180" />
  &nbsp;&nbsp;
  <img src="assets/bubble-empty.png" alt="空态猫头鹰悬浮球" width="180" />
</p>

**尊重阈值的通知** —— 低额度告警（默认 20%，10% 升级）、余额下限、重置提醒、认证过期告警、连续网络失败检测 —— 每条都经过去重，绝不轰炸。

**用量分析** —— 26 周 GitHub 风格热力图逐日呈现 Tokens 与计费，30 天趋势图，按供应商的 2 小时 / 24 小时 / 30 天分段视图，以及预估额度耗尽时间的燃烧速率推演。

<p align="center">
  <img src="assets/history.png" alt="历史视图：30 天统计、趋势图与 26 周热力图" width="820" />
</p>

**网关友好** —— 自托管 New API、One API 或 LiteLLM？它们都是一等内置项，网关地址可自由编辑。自定义 OpenAI 兼容端点同样支持。

**自动更新** —— 发布产物经 Minisign 签名；内置更新器安装前先验证签名。

## 支持的供应商

29 个内置清单横跨三种请求协议，按各家 API 实际暴露的能力如实分级 —— 而不是按 UI 想展示什么来分级：

| 等级 | 能看到什么 | 供应商 |
| :--- | :--- | :--- |
| **L4** — 完整额度 | 额度 + 用量 + 余额 + 重置 + 账户标识 | OpenAI、OpenRouter |
| **L3** — 用量 / 余额 | 用量或余额可读 | DeepSeek、Qwen、Kimi、SiliconFlow |
| **L2** — 健康 / 模型 | 连接健康、模型列表、限流响应头 | Anthropic、Gemini、Groq、Mistral、xAI、智谱、豆包、百川、MiniMax、StepFun、零一万物、360、讯飞、百度、腾讯、小米 MiMo、Cohere… |
| **L2** — 网关 | 你的部署、你的地址 | New API、One API、LiteLLM |
| **L1** — 自定义 | 任意 OpenAI 兼容端点 | *自定义 / 兼容* |

原生 Anthropic 与 Google Gemini 协议适配器已内置（超越其 OpenAI 兼容层）；**社区供应商包** —— 将通过校验的 YAML 清单放入 `community-providers` 目录即可注册新供应商，无需改代码。

> 缺少你用的供应商？任何 OpenAI 兼容 API 今天就能走*自定义*路径接入，写一个[社区包](#社区供应商包)也只需五分钟。

## 安装

> **说明：** Windows 构建已端到端验证。macOS 产物由 CI 构建但尚未在真实硬件上验证 —— 请当作早期版本对待。

### Windows（已验证）

从 [Releases](https://github.com/Aswellle/TokenOwl/releases) 获取最新安装包：

- `TokenOwl_x.y.z_x64-setup.nsis.exe` —— 推荐安装器
- `TokenOwl_x.y.z_x64_en-US.msi` —— 企业部署用 MSI

### macOS（CI 构建，未在真机验证）

- `TokenOwl_x.y.z_aarch64.dmg` —— Apple Silicon
- `TokenOwl_x.y.z_x64.dmg` —— Intel

## 快速上手

1. 启动 TokenOwl —— 悬浮球自动吸附到最近的屏幕边缘。
2. 点击悬浮球，进入**设置 → 连接账户**。选择供应商，粘贴 API 密钥，端点会按内置清单自动填充。
3. 数据数秒内到达，悬浮球双环点亮。
4. 拖动悬浮球到任意位置 —— 它会吸附到边缘，并记住你放的地方。

密钥直接进入 Windows 凭据管理器（macOS 上是钥匙串）。SQLite 数据库只存密钥的*引用*，绝不存密钥本身。

## 工作原理

一套刻意朴素、分层清晰的架构：webview 只负责渲染，其余一切都由 Rust 完成。

```
┌────────────────────────────────────────────────────────┐
│  Webview（React 19 + TypeScript strict + motion）      │
│  仪表盘 · 悬浮球 · 历史 · 通知                          │
│  —— 按能力策略，无任何网络权限                           │
└──────────────────────────┬─────────────────────────────┘
                           │ 仅语义化 IPC 命令
┌──────────────────────────▼─────────────────────────────┐
│  Rust 核心（Tauri 2）                                   │
│                                                        │
│  scheduler ──► providers ──► domain                    │
│  抖动 +        适配器 +      快照 · 重置计算             │
│  退避          清单          健康分级 · 燃烧速率        │
│      │             │                                    │
│      ▼             ▼                                    │
│  storage         networking        credentials          │
│  SQLite WAL      超时 ·            仅系统凭据库          │
│  版本化迁移       大小上限          （磁盘只存引用）      │
└────────────────────────────────────────────────────────┘
```

值得了解的设计决策：

- **Webview 无法发起网络请求。** Tauri 能力集只授予 core、opener、process、updater。所有 HTTP 都发生在 Rust 侧，经由一个共享客户端完成，带超时、响应大小上限与严格 CSP。被攻破的 webview 到你的 API 密钥之间没有通路。
- **清单驱动的供应商。** 每个内置供应商都是一份声明式 YAML（`providers/builtin/*.yaml`），描述其请求协议、端点与如实的能力等级。新增供应商是数据问题，不是代码问题 —— 社区包在运行时扩展同一机制。
- **按指标故障隔离。** 单个端点失败降级为该指标上的错误徽标，其余端点继续更新。整体故障时保留上一个可用快照、标记为 stale，并按有界指数退避重试。UI 永不冻结，也绝不悄悄归零。
- **版本化 SQLite 迁移**，带完整性检查、迁移前自动备份与逐迁移事务。

## 参与开发

### 环境要求

- Node.js 22+ 与 pnpm 10.33.2（`corepack enable`）
- Rust stable 1.85+（2024 edition）
- 各平台 [Tauri 2 前置依赖](https://v2.tauri.app/start/prerequisites/)

```bash
git clone https://github.com/Aswellle/TokenOwl.git
cd TokenOwl
corepack enable
pnpm install                     # 工作区安装（仓库根目录）
```

### 日常命令（在 `apps/desktop/` 下运行）

```bash
pnpm dev                         # Vite :1420，浏览器预览 UI
pnpm tauri dev                   # 完整桌面应用 + 热重载
pnpm typecheck                   # tsc --noEmit（严格模式）
pnpm test                        # vitest, jsdom
pnpm exec playwright test        # e2e 冒烟套件
```

```bash
cd apps/desktop/src-tauri
cargo test                       # 219 个单元 + 集成测试
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all
```

### 项目结构

```
apps/desktop/
├── src/                 # React 前端（features/, components/, lib/）
├── src-tauri/
│   └── src/
│       ├── domain/      # 快照、重置计算、健康分级、历史
│       ├── providers/   # 适配器 trait、内置适配器、清单引擎
│       ├── scheduler/   # 优先级队列、抖动、指数退避
│       ├── storage/     # SQLite WAL、版本化迁移
│       ├── credentials/ # 系统凭据库封装（磁盘只存引用）
│       ├── networking/  # 共享 HTTP 客户端（限制、脱敏）
│       └── commands/    # Tauri IPC 处理器
├── providers/builtin/   # 29 份 YAML 供应商清单
assets/                   # 本自述文件使用的截图
```

### 测试理念

供应商解析由离线确定性 fixture 覆盖 —— 成功、部分字段、畸形 JSON、401、带 `retry-after` 的 429、5xx、空响应体，以及重置窗口边界情形。密钥处理有专门的回归套件，逐字节扫描 SQLite 文件、日志与序列化 DTO（含阳性对照）。UI 状态迁移、新鲜度回退与退避上限各有专属测试。

## 社区供应商包

将 YAML 清单放入 `<应用数据目录>/community-providers/` 后重启：

```yaml
id: my-relay
version: 1
name: My Relay
auth: [api_key]
baseUrl: https://relay.example.com/v1
apiKeyHeader: Authorization
endpoints:
  models: /models
  balance: /credits
apiFormat: openai-compatible
```

加载器刻意从严：清单必须完整解析、使用 `openai-compatible` 协议、声明**https** 基础地址且不内嵌凭据、id 不得与内置项冲突。无效的包会被跳过并在设置页报告原因 —— 绝不半注册。

## 安全

- **密钥只存系统凭据库** —— SQLite 只存 `credential_ref`；凭据类型不可序列化，`Debug` 输出为 `REDACTED`。
- **网络受限的 webview** —— 无网络权限、严格 CSP；所有 HTTP 在 Rust 中完成，HTTPS 优先的 URL 校验、大小上限与敏感头脱敏。
- **密钥泄漏回归套件** —— 逐字节扫描数据库、日志与 DTO，证明密钥绝不会明文落盘。
- **签名更新** —— 更新器产物经 Minisign 验证；未签名或改签的发布无法安装。

发现漏洞？请使用[私密安全通告](https://github.com/Aswellle/TokenOwl/security/advisories/new)而非公开 issue —— 详见 [SECURITY.md](SECURITY.md)。

## 贡献

欢迎 issue 与 PR —— 适合上手的有新供应商清单（一份 YAML 加 fixture）与 UI 打磨。工作流见 [CONTRIBUTING](CONTRIBUTING.md)，各供应商等级的含义见[能力清单目录](providers/builtin/)，架构细节见代码内 Rustdoc。

## 许可证

[MIT](LICENSE) © 2026 TokenOwl contributors

---

<p align="center">
  <sub>基于 <a href="https://v2.tauri.app/">Tauri 2</a> · <a href="https://react.dev">React 19</a> · <a href="https://www.rust-lang.org">Rust</a> 构建 —— 无云端、无遥测、无账户。</sub>
</p>
