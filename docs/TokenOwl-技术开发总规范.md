# TokenOwl 技术开发总规范

> 本文是 TokenOwl 项目的总技术开发约束。AI Agent 执行开发时应先阅读本文，再阅读 `docs/` 下各专题文档。

## 产品愿景

TokenOwl 是一个 **Local-first AI Usage & Quota Desktop Monitor**。

它的目标不是成为另一个 AI 网关，而是成为用户所有 AI 账户的一个低干扰、长期常驻的“额度雷达”。

用户无需打开十几个后台，就可以从一个 50–60px 的悬浮球得到三个答案：

1. 现在有没有额度问题？
2. 还剩多少？
3. 什么时候刷新？

点击后进入仪表盘，可进一步查看所有账户、5h/7d/monthly quota、bonus/free credits、余额、历史使用、provider 状态和数据来源。

## 技术结论

### Desktop

**Tauri 2 + React + TypeScript + Rust**。

### Data

**SQLite + WAL + migrations**。

### Secret

**Windows Credential Manager / macOS Keychain**。

### Provider

**Rust Provider Trait + Declarative Manifest + Future WASM Adapter**。

### Custom Gateway

**OpenAI-compatible first-class support**，以适配 New API / One API / LiteLLM / 私人中转站。

### Future Mobile

Rust domain/application core 与 UI 解耦，为 Tauri mobile 或其他移动前端复用留下清晰边界。

## 核心架构

```text
                    TokenOwl UI
        ┌───────────────┴───────────────┐
        │                               │
  Floating Bubble                  Dashboard
        │                               │
        └───────────────┬───────────────┘
                        │ IPC/Event
                  Application Layer
                        │
                  Domain Core
          ┌─────────────┼─────────────┐
          │             │             │
      Provider       Scheduler     Persistence
       Registry                       │
          │                       SQLite
    ┌─────┴─────┐                    │
 Builtin      Manifest          Credential Ref
 Adapter      Adapter                │
    │             │            OS Keychain
    └─────┬───────┘
          │
     HTTP/OAuth/Web/Local
```

## 最重要的设计原则

### 1. Quota != Token

支持：

```text
TOKEN
CURRENCY
REQUEST
CREDIT
POINT
COMPUTE_UNIT
CHARACTER
MINUTE
IMAGE
UNKNOWN
```

### 2. 数据没有就说没有

如果 Provider 没有提供 quota：

> `Connected — quota unavailable`

不要自行制造一个“100%”。

### 3. reset 必须可信

优先使用 Provider 给出的 `reset_at`。

只有在规则明确且可验证时才能推导 reset。

### 4. 网络不应该阻塞产品

Provider 请求是在后台执行。

UI 永远基于：

```text
last known good snapshot
```

并展示 freshness。

### 5. Provider 不应该污染 UI

新 Provider 不应该让 Agent 修改 Dashboard Card 代码。

---

## 首发 Provider 思路

国际：

```text
OpenAI
Anthropic
Gemini
OpenRouter
DeepSeek
xAI
Mistral
Groq
Cohere
Azure OpenAI
```

中国大陆/亚洲：

```text
Alibaba/Qwen
Doubao/Volcengine
GLM/Zhipu
Kimi/Moonshot
MiniMax
Baidu/Qianfan
iFlytek
Tencent Hunyuan
SiliconFlow
StepFun
01.AI
Baichuan
360
Xiaomi MiMo
Coze
```

网关/私人部署：

```text
New API
One API
LiteLLM
OpenAI-compatible
Custom JSON API
```

实际能力以每个 Provider 的 `capabilities` 与 fixture 为准。

---

## 开源复用

重点参考：

1. `steipete/CodexBar`：Provider/quota/reset/source 产品经验；当前仓库页面显示 MIT，并列出了 Alibaba Coding Plan、Alibaba Token Plan、Qwen Cloud、Kimi、MiniMax、Doubao、DeepSeek、Moonshot、Xiaomi MiMo 等多个相关 Provider。
2. `ccusage/ccusage`：本地 AI coding usage 与 5-hour blocks。
3. `BerriAI/litellm`：100+ Provider、模型与 pricing 生态参考。
4. `songquanpeng/one-api`：中国大陆 Provider 与 OpenAI-compatible channel 参考。
5. `QuantumNous/new-api`：网关兼容、渠道/配额领域参考；当前仓库为 AGPL-3.0，不应未经合规审查直接复制代码。
6. `anomalyco/models.dev`：模型规格、价格、Provider metadata。

具体规则见 `docs/08-开源能力复用与许可证.md`。

---

## Agent 开发约束

每次开发遵循：

```text
Baseline
 ↓
Design
 ↓
Implement
 ↓
Test
 ↓
Build
 ↓
UX Verify
 ↓
Diff Audit
 ↓
Implementation Report
```

每个阶段必须可运行。

Agent 不允许：

- 为了新功能删除旧功能；
- 不跑测试就提交；
- 不做 cross-platform build 就声称完成；
- 把真实账户 response 当成测试；
- 把 secret 写入日志；
- 将 quota provider 代码塞进 React；
- 把所有错误归类成 unknown；
- 用假数据“填满” UI。

---

## 最终工程目标

TokenOwl 的成熟形态应达到：

```text
40+ Provider
        ↓
统一 Provider Protocol
        ↓
统一 Quota Domain
        ↓
统一 Reset Engine
        ↓
统一 History / Notification
        ↓
统一 Floating Desktop UX
```

同时支持：

```text
Official API
OAuth
Browser/Web source
Local CLI
Custom endpoint
OpenAI-compatible gateway
Manifest provider
未来 WASM provider
```

并保持：

```text
Local-first
Secure
Fast
Silent
Cross-platform
Extensible
```

---

## 文档执行优先级

```text
07-AI-Agent开发执行规范.md
        ↓
01-PRD.md
        ↓
02-技术架构.md
03-Provider接入协议.md
04-数据模型与内部API.md
05-UIUX设计规范.md
06-安全隐私测试与发布.md
08-开源能力复用与许可证.md
09-Provider能力矩阵.md
10-实施路线图与验收清单.md
```
