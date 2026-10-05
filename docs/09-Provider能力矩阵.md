# TokenOwl Provider 初始能力矩阵

> 这张表是“产品首发规划”，不是对所有供应商当前接口能力的永久承诺。上线前需要由 Provider adapter 的实际 fixture 和官方文档重新验证。

| Provider | 账户类型 | 额度单位 | 目标窗口 | 首发方式 | 优先级 |
|---|---|---|---|---|---|
| OpenAI | API / usage | currency/token | monthly/custom | Official API | P0 |
| Anthropic | subscription/API | token/window | 5h/7d/usage | Official/OAuth/source | P0 |
| Gemini | subscription/API | quota/token | provider-defined | Official/OAuth | P0 |
| OpenRouter | prepaid/API | currency/token | balance/usage | Official API | P0 |
| DeepSeek | prepaid/API | currency/token | balance/usage | Official API | P0 |
| Qwen / Alibaba | Coding/Token/API | token/credit | 5h/7d/balance | official/web/API | P0 |
| Kimi / Moonshot | subscription/API | token/balance | 5h/7d/balance | official/API | P0 |
| MiniMax | coding/API | token/credit | provider-defined | official/API/source | P0 |
| Zhipu / GLM | subscription/API | quota/token | provider-defined | official/API | P1 |
| Doubao / Volcengine | API/Coding | request/token | provider-defined | official/API | P1 |
| Mistral | API/subscription | credit/currency | monthly/balance | official API | P1 |
| Groq | API | request/token/rate | hourly/provider-defined | official API | P1 |
| xAI | API | currency/token | billing | official API | P1 |
| Cohere | API | usage | billing | official API | P1 |
| SiliconFlow | API | currency/token | balance/usage | official API | P1 |
| StepFun | API/subscription | credit/token | provider-defined | official/API | P1 |
| Baidu Qianfan | API | currency/token | billing | official API | P1 |
| iFlytek | API | quota/points | provider-defined | official API | P2 |
| Tencent Hunyuan | API | token/credit | provider-defined | official API | P2 |
| Baichuan | API | token/balance | billing | official API | P2 |
| 01.AI | API | token/balance | billing | official API | P2 |
| Xiaomi MiMo | subscription/API | usage/credits | provider-defined | official/source | P2 |
| 360 | API | credit | provider-defined | official API | P2 |
| Coze | API/platform | credits/usage | provider-defined | official API | P2 |
| New API | gateway | custom | custom | Generic API | P0 |
| One API | gateway | custom | custom | Generic API | P0 |
| LiteLLM | gateway | custom | custom | Generic API | P0 |
| Custom OpenAI | custom | custom | custom | Manifest | P0 |

## 能力等级

```text
L4 Full Quota
  quota + usage + reset + account identity

L3 Usage/Balance
  usage 或 balance 可读，reset 不完整

L2 Health/Model
  models + connection health

L1 Custom
  用户手动提供 endpoint / parser
```

## 重要实现规则

任何没有公开账户 quota 的 Provider，不允许因为产品 UI 需要一条进度条而推断一个假的上限。

正确显示：

```text
Connected
Balance unavailable through supported API
```

而不是：

```text
100 / 100
```
