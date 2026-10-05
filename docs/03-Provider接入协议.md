# TokenOwl Provider Integration Protocol

## 1. 设计目标

TokenOwl 的核心不是“拥有很多供应商代码”，而是拥有一个稳定、可扩展、可验证的 Provider Contract。

新增 Provider 后必须满足：

```text
Provider code/config
      ↓
Registry
      ↓
Generic Application Layer
      ↓
Generic UI
```

不能新增一个 Provider 就去修改 Dashboard、Bubble、Settings 三个地方。

---

## 2. Provider 能力模型

```ts
export type ProviderCapabilities = {
  identity: boolean;
  balance: boolean;
  usage: boolean;
  quota: boolean;
  resetAt: boolean;
  modelCatalog: boolean;
  pricing: boolean;
  oauth: boolean;
  apiKey: boolean;
  browserSession: boolean;
  localSource: boolean;
  openAiCompatible: boolean;
};
```

能力必须是真实声明，不支持就返回 false。

---

## 3. Adapter Contract

逻辑接口：

```rust
#[async_trait]
pub trait ProviderAdapter: Send + Sync {
    fn descriptor(&self) -> ProviderDescriptor;

    async fn validate_account(
        &self,
        account: &AccountContext,
    ) -> ProviderResult<AccountIdentity>;

    async fn fetch_quota(
        &self,
        account: &AccountContext,
        ctx: &FetchContext,
    ) -> ProviderResult<QuotaSnapshotSet>;

    async fn fetch_usage(
        &self,
        account: &AccountContext,
        range: UsageRange,
        ctx: &FetchContext,
    ) -> ProviderResult<Vec<UsagePoint>>;

    async fn health_check(
        &self,
        account: &AccountContext,
        ctx: &FetchContext,
    ) -> ProviderResult<HealthStatus>;
}
```

Provider 不负责 UI。

---

## 4. Provider Descriptor

```rust
pub struct ProviderDescriptor {
    pub id: String,
    pub display_name: String,
    pub region: ProviderRegion,
    pub auth_methods: Vec<AuthMethod>,
    pub capabilities: ProviderCapabilities,
    pub default_refresh: Duration,
    pub docs_url: Option<String>,
}
```

---

## 5. QuotaSnapshot

```rust
pub struct QuotaSnapshot {
    pub id: Uuid,
    pub account_id: Uuid,
    pub metric_id: String,
    pub metric_name: String,
    pub unit: QuotaUnit,
    pub used: Option<Decimal>,
    pub limit: Option<Decimal>,
    pub remaining: Option<Decimal>,
    pub window: Option<QuotaWindow>,
    pub observed_at: DateTime<Utc>,
    pub source: DataSource,
    pub confidence: Confidence,
    pub status: SnapshotStatus,
}
```

### 为什么全部使用 Option

因为真实 Provider 可能只给：

- `remaining`；
- `used`；
- `limit` + `used`；
- 单独 balance；
- resetAt；
- 或只给一个百分比。

TokenOwl 不应伪造缺失字段。

---

## 6. QuotaWindow

```rust
pub enum WindowType {
    Rolling,
    Fixed,
    Calendar,
    Subscription,
    Custom,
}

pub struct QuotaWindow {
    pub window_type: WindowType,
    pub duration_seconds: Option<i64>,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
    pub reset_at: Option<DateTime<Utc>>,
}
```

---

## 7. DataSource

```text
OFFICIAL_API
OAUTH_API
BROWSER_SESSION
COOKIE
CLI_LOCAL
LOCAL_DB
CUSTOM_ENDPOINT
MANUAL
CACHED
```

UI 必须展示 source。

例如：

> `Updated 2m ago · OAuth API`

而不是只写：

> `Updated`

---

## 8. Confidence

```text
EXACT
DERIVED
ESTIMATED
STALE
UNKNOWN
```

例如：

- Provider 返回 `reset_at` → EXACT
- 本地推导固定月度边界 → DERIVED
- 从 usage 估算 burn rate → ESTIMATED
- 使用旧 snapshot → STALE

---

## 9. Declarative Manifest Adapter

大量普通 OpenAI-compatible / JSON API 可以不写 Rust。

示例：

```yaml
id: custom.openai-compatible
version: 1
name: Custom OpenAI-Compatible
category: gateway

auth:
  - api_key

endpoints:
  models:
    method: GET
    path: /v1/models
    auth: api_key

  balance:
    method: GET
    path: /balance
    auth: api_key

  usage:
    method: GET
    path: /usage
    auth: api_key

mapping:
  balance:
    value: $.data.balance
    unit: currency
    currency: USD

  usage:
    used: $.data.total_tokens
    unit: token

network:
  timeout_ms: 10000
  follow_redirects: false
```

---

## 10. Manifest 安全约束

Manifest 不允许：

- 执行任意 shell；
- 读取任意本地文件；
- 访问环境变量；
- 加载动态 native library；
- 修改系统设置；
- 将 secret 发送到未声明的 endpoint；
- 自动把 Cookie 上传到第三方服务。

Manifest 可以：

- 发起 HTTPS 请求；
- 在显式授权后访问 HTTP/LAN endpoint；
- 从 Credential Manager 获取指定 secret；
- 对 JSON 做受限字段映射。

---

## 11. 第三方 Native/WASM Adapter

### Phase 1

只允许内置 Rust Adapter。

### Phase 2

Manifest Adapter。

### Phase 3

WASM Adapter：

```text
Provider Manifest
     ↓
Signed WASM Package
     ↓
Sandbox
     ├── http.request
     ├── credential.get(ref)
     └── clock.now()
```

默认没有 filesystem/process/network arbitrary capability。

建议未来建立：

```text
Provider Package
├─ manifest.json
├─ adapter.wasm
├─ README.md
├─ LICENSE
├─ icon.svg
└─ checksums.txt
```

---

## 12. Custom Endpoint

必须支持：

### Base URL

```text
https://api.example.com
https://gateway.example.com/v1
http://192.168.1.20:3000/v1
```

### 请求

支持：

- GET
- POST
- query params
- JSON body template
- headers
- Bearer
- X-API-Key
- Basic

### LAN 访问

第一次使用局域网地址时必须显式提示：

> 此 Provider 指向局域网地址。仅在你确认该地址属于自己的网关或可信网络时继续。

---

## 13. Provider 最小测试集

每个 Adapter 至少包含：

```text
descriptor.test
auth.test
success_response.test
partial_response.test
reset_window.test
unit_mapping.test
rate_limit.test
timeout.test
malformed_json.test
stale_fallback.test
secret_redaction.test
```

对 Provider API response 使用真实 fixture，不依赖 CI 时访问真实账户。

---

## 14. 首发 Provider Adapter 分组

### Group A：必须完整

- OpenAI
- Anthropic
- Gemini
- OpenRouter
- Qwen / Alibaba
- DeepSeek
- Kimi / Moonshot
- MiniMax

### Group B：重要

- Zhipu / GLM
- Doubao / Volcengine
- Mistral
- Groq
- SiliconFlow
- StepFun
- xAI
- Cohere
- Azure OpenAI

### Group C：持续扩充

- Baidu / Qianfan
- iFlytek
- Tencent Hunyuan
- Baichuan
- 01.AI
- 360
- Xiaomi MiMo
- Coze
- 私有 New API
- 私有 One API
- 私有 LiteLLM
- 任意 OpenAI-compatible endpoint

---

## 15. Provider Registry

```rust
pub struct ProviderRegistry {
    adapters: HashMap<String, Arc<dyn ProviderAdapter>>,
    manifests: HashMap<String, ProviderManifest>,
}

impl ProviderRegistry {
    pub fn register(&mut self, adapter: Arc<dyn ProviderAdapter>);
    pub fn register_manifest(&mut self, manifest: ProviderManifest);
    pub fn get(&self, id: &str) -> Option<ProviderHandle>;
}
```

Provider 注册与 UI 完全解耦。

---

## 16. Provider 版本与兼容性

```text
provider_schema_version
adapter_version
api_version
min_app_version
max_app_version
```

发生 Provider API 变化时，不能直接破坏已有账户；应该将其标记为 `PROVIDER_ERROR` 或 `UNSUPPORTED`，保留旧 snapshot。

---
