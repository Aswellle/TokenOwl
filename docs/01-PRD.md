# TokenOwl PRD — AI 账户额度桌面监视器

## 1. 产品定义

### 1.1 产品名称

**TokenOwl**

### 1.2 一句话定位

> 一个常驻桌面的 AI 额度雷达：不用打开多个网页、不用切换多个后台，即可快速知道你的 AI 账户还剩多少额度、何时重置、过去消耗如何，以及当前连接是否正常。

### 1.3 核心用户价值

TokenOwl 解决的问题不是“调用 AI”，而是“管理我已经拥有的 AI 能力”。用户可能同时拥有：

- 官方 API 平台账户；
- ChatGPT / Claude / Gemini / Kimi / Qwen / MiniMax 等订阅或 Coding Plan；
- 按金额预付费的 API 账户；
- 免费赠送 credits / points / request；
- OpenRouter、SiliconFlow 等聚合平台；
- 自建 New API / One API / LiteLLM / OpenAI-compatible 中转站；
- 私人搭建的兼容 API endpoint。

用户真正需要的不是几十个不同的后台页面，而是一个统一的“**剩余能力视图**”。

---

## 2. 产品目标

### 2.1 P0 目标

1. 30 秒内完成第一个账户接入。
2. 常驻悬浮球始终显示核心健康状态。
3. 一次展开即可看到所有已配置账户的额度与重置信息。
4. 支持 token、金额、请求数、credit、point、compute unit 等多种额度单位。
5. 支持 5h、7d、daily、weekly、monthly、rolling、fixed、自定义 reset window。
6. Provider 数据刷新在后台进行，不阻塞 UI。
7. Provider 失败后继续显示上次成功数据并明确显示数据新鲜度。
8. 新 Provider 可以通过适配器或声明式配置扩展，不修改核心 UI。
9. Windows 与 macOS 体验一致。
10. 供应商 API Key 等敏感凭据仅使用系统安全凭据存储。

### 2.2 非目标

- 不成为 LLM API Gateway。
- 不代理、转发、缓存用户的实际模型请求。
- 不代替供应商 Billing 系统进行最终计费结算。
- 不宣称可以读取所有供应商的隐藏限额。
- 不自动突破或绕过供应商认证、验证码、访问限制。
- 不要求 TokenOwl 云端账户才能使用基础功能。

---

## 3. 用户类型

### U1：AI 重度开发者

同时使用 OpenAI、Claude、Gemini、DeepSeek、Qwen、Kimi 等，希望在写代码时快速知道“哪个额度快没了”。

### U2：多订阅用户

拥有多个 AI Coding Plan，希望随时查看 5h/7d/月度窗口。

### U3：API 消费用户

拥有多个按金额计费的 API Key，需要余额、月度消费和阈值预警。

### U4：中转站/网关用户

使用自建 New API、One API、LiteLLM 或私人 OpenAI-compatible endpoint，希望 TokenOwl 能通过自定义 API 获取余额/usage/quota。

### U5：高级用户 / 第三方适配维护者

希望自己编写 Provider Adapter 或 Manifest，而不是等待 TokenOwl 官方发布版本。

---

## 4. 核心信息架构

```text
Floating Bubble
     │
     ├── Quick status
     │
     └── Expand
          │
          ├── Overview
          │    ├── All Accounts
          │    ├── Critical Quotas
          │    └── Next Resets
          │
          ├── Provider Detail
          │    ├── Usage
          │    ├── Quota Windows
          │    ├── Credit / Grant
          │    ├── Account identity
          │    └── Last sync / source
          │
          ├── History
          │    ├── Token
          │    ├── Cost
          │    ├── Requests
          │    └── Reset cycles
          │
          ├── Providers
          │    ├── Built-in
          │    ├── Custom
          │    └── Installed adapters
          │
          └── Settings
               ├── Refresh
               ├── Notifications
               ├── Appearance
               ├── Privacy
               └── About
```

---

## 5. 浮窗形态

### 5.1 折叠态

默认尺寸建议 **56 × 56 px**，允许用户设置 48–72 px。

视觉要求：

- 圆形。
- 默认无文字。
- 中心显示 TokenOwl Owl 图标。
- 外环显示综合状态。
- 不主动闪烁。
- 支持拖动。
- 支持边缘吸附。
- 支持记忆最后位置。
- 支持显示/隐藏悬浮球。
- 双击可展开。
- 单击展开或根据设置执行动作。
- 右键显示菜单。

综合状态计算建议：

```text
Healthy      → 所有关键 Provider 正常
Attention    → 任一窗口剩余 < 25%
Critical     → 任一关键窗口剩余 < 10%
Stale        → 所有最新数据超过阈值
Error        → 没有可用快照且 Provider 请求失败
Syncing      → 正在首次刷新
```

### 5.2 展开态

推荐基础尺寸约 **720 × 680 px**；允许根据屏幕、DPI 和用户设置自适应。

动画从圆形平滑变为 Panel，使用 180–240ms 的 ease-out / spring-like transition；避免缩放造成文字模糊，可将浮窗容器与内部内容分层动画。

### 5.3 Dashboard 快速视图

顶部：

- 总账户数。
- 正常 / 注意 / 异常数量。
- 最近一次同步。
- 全局刷新按钮。

中部 Provider Card：

```text
┌────────────────────────────────────────┐
│  Qwen / Coding Plan             ● OK   │
│                                        │
│  5h window                 72% left    │
│  ████████████████████░░░░     3h 12m   │
│                                        │
│  7d window                 41% left    │
│  ███████████░░░░░░░░░░░░     2d 04h   │
│                                        │
│  Bonus credits             680 / 1000 │
│  ───────────────────────────────────   │
│  Last updated 16:42 · API              │
└────────────────────────────────────────┘
```

---

## 6. 额度模型

TokenOwl 禁止将额度硬编码为 token。

统一 `QuotaUnit`：

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

统一窗口类型：

```text
ROLLING        # 滑动窗口，例如过去 5 小时
FIXED          # 固定时间窗口
CALENDAR       # 日/月/账单周期
SUBSCRIPTION   # 套餐周期
NO_WINDOW      # 单纯余额
CUSTOM
```

统一账户类型：

```text
SUBSCRIPTION
API_PAYG
PREPAID
FREE_GRANT
TEAM
ORGANIZATION
CUSTOM
```

---

## 7. 5 小时 / 7 天 Reset 产品规则

### 7.1 必须显示

- 当前剩余量。
- 总额度（如果供应商提供）。
- 已使用量。
- 使用率。
- reset 时间。
- 距离 reset 的时间。
- 窗口开始时间 / 结束时间（如果可得）。
- 数据时间戳。

### 7.2 Reset 倒计时

倒计时必须以 Provider 返回的 `reset_at` 为准。

不能因为看到“5h quota”就强制推断 5 小时后的 reset 时间，除非该 Provider 已明确声明为固定/滚动窗口且可以安全推导。

### 7.3 Reset 进度

同时呈现两个概念：

1. **Quota consumption progress**：用户用了多少。
2. **Window elapsed progress**：距离本窗口结束还有多久。

避免把二者混成一条 bar。

---

## 8. 不同免费赠送额度 / Bonus Plan

Provider 可以同时返回：

```text
Paid Balance
Free Grant
Subscription Quota
Bonus Credits
Promo Credits
Team Pool
```

UI 中必须保持桶的独立性；不得直接把不同 bucket 相加成“伪余额”。

显示示例：

```text
Free Grant       680 / 1000 credits
Paid Balance     $12.40
5h Subscription  23% used
7d Subscription  59% used
```

---

## 9. Provider 首发范围

### 9.1 国际/跨区域

首发优先：

- OpenAI
- Anthropic / Claude
- Google Gemini
- OpenRouter
- xAI
- Mistral
- Groq
- Cohere
- DeepSeek
- Azure OpenAI

### 9.2 中国大陆及亚洲优先

首发优先：

- 阿里云百炼 / Qwen / Alibaba Coding Plan / Token Plan
- 火山引擎方舟 / 豆包
- 智谱 GLM / BigModel
- 月之暗面 / Kimi / Moonshot
- MiniMax
- DeepSeek
- 百度千帆 / 文心
- 讯飞星火
- 腾讯混元
- SiliconFlow / 硅基流动
- StepFun / 阶跃星辰
- 01.AI / 零一万物
- Baichuan / 百川
- 360 智脑
- 小米 MiMo
- Coze

注意：这里的“支持”必须拆成能力矩阵，而不是一概宣称“可读取订阅额度”。

能力等级：

```text
L4 Full Quota       官方接口直接获取 quota + reset + usage
L3 Usage/Balance    可获得 usage 或 balance，但缺少部分 reset
L2 Health/Model     只能做连通/模型探测等有限能力
L1 Custom            用户自己提供 endpoint / parser
```

---

## 10. 自定义中转站

必须提供 `Custom OpenAI-Compatible`。

最小配置：

```text
Name
Base URL
API Key
Models endpoint (optional)
Balance endpoint (optional)
Usage endpoint (optional)
Quota endpoint (optional)
Custom headers (optional)
Authentication type
Parser strategy
```

允许：

- `/v1/models`
- `/v1/usage`
- `/usage`
- `/balance`
- `/quota`
- `/billing`
- 自定义 endpoint

并提供 JSONPath/JMESPath/模板映射。

---

## 11. Provider 状态

```text
CONNECTED
SYNCING
STALE
AUTH_REQUIRED
RATE_LIMITED
UNSUPPORTED
NETWORK_ERROR
PARSER_ERROR
PROVIDER_ERROR
DISABLED
```

UI 必须区分：

- “Provider 没有额度 API”；
- “TokenOwl 暂时无法连接”；
- “认证过期”；
- “供应商接口返回错误”；
- “成功取得旧数据”；

不能统一显示成“未知错误”。

---

## 12. 通知

可选，不打扰为原则。

触发条件：

- 5h < 20%
- 7d < 20%
- Balance < 用户设定金额
- reset 即将发生
- Provider 长时间失去连接
- OAuth / API key 即将过期（仅在明确可判断时）

支持：

- Windows Notification
- macOS Notification
- 静默模式
- 只提醒 Critical

---

## 13. 历史数据

### MVP

- 当前 snapshot。
- 最近 7/30 天的 usage。
- 每个 quota window 的变化。

### V2

- provider 对比。
- 模型消耗。
- 每日/每周成本。
- burn rate。
- 估算“按当前速度还能使用多久”（仅作为估算，明确标记）。

---

## 14. 产品质量目标

| 指标 | 目标 |
|---|---:|
| 冷启动 | ≤ 2s 进入可交互 UI（硬件允许情况下） |
| 浮窗展开 | 视觉响应 < 100ms |
| expand/collapse 动画 | 180–240ms |
| 空闲 CPU | ≤ 1%（目标值） |
| 空闲内存 | ≤ 120MB（目标值，随 WebView/系统差异允许偏差） |
| 单 Provider 刷新超时 | 默认 8–12s，可配置 |
| UI 被网络请求阻塞 | 0 |
| Secret 出现在普通日志 | 0 |
| Provider 单点失败导致全局失败 | 0 |

这些是工程目标而不是硬件无关的绝对保证，必须通过实际构建机基线测试校准。

---

## 15. MVP Definition of Done

MVP 必须完成：

- [ ] Windows x64 可安装。
- [ ] macOS Apple Silicon 可安装。
- [ ] macOS Intel 可安装（若仍有发布目标）。
- [ ] 悬浮球可拖动。
- [ ] 展开 / 折叠。
- [ ] 自动记忆位置。
- [ ] 开机自启可选。
- [ ] Tray Menu。
- [ ] SQLite 本地缓存。
- [ ] OS Keychain。
- [ ] Provider Adapter Protocol。
- [ ] Custom OpenAI-Compatible Provider。
- [ ] 至少 6 个国际/国内真实 Provider。
- [ ] 5h/7d 至少各实现一个真实 Provider。
- [ ] balance/credit 至少各实现一个 Provider。
- [ ] Stale fallback。
- [ ] 基础历史图表。
- [ ] 全套自动化测试。
- [ ] CI Windows/macOS 构建。

---
