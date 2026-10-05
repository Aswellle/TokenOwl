# TokenOwl 数据模型与内部 API

## 1. 核心实体

```text
Provider
   │
   └── Account
         │
         ├── CredentialRef
         ├── QuotaSnapshot[]
         └── UsagePoint[]
```

---

## 2. Provider

```sql
CREATE TABLE providers (
  id TEXT PRIMARY KEY,
  display_name TEXT NOT NULL,
  schema_version INTEGER NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 1,
  region TEXT,
  adapter_version TEXT,
  capabilities_json TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
```

---

## 3. Account

```sql
CREATE TABLE accounts (
  id TEXT PRIMARY KEY,
  provider_id TEXT NOT NULL,
  label TEXT NOT NULL,
  account_type TEXT NOT NULL,
  credential_ref TEXT,
  metadata_json TEXT,
  enabled INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY(provider_id) REFERENCES providers(id)
);
```

禁止在 `metadata_json` 放：

- API Key
- bearer token
- refresh token
- Cookie
- session cookie
- password

---

## 4. Quota Snapshot

```sql
CREATE TABLE quota_snapshots (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL,
  metric_id TEXT NOT NULL,
  metric_name TEXT NOT NULL,
  unit TEXT NOT NULL,
  currency TEXT,
  used REAL,
  limit_value REAL,
  remaining REAL,
  window_type TEXT,
  window_start TEXT,
  window_end TEXT,
  reset_at TEXT,
  observed_at TEXT NOT NULL,
  source TEXT NOT NULL,
  confidence TEXT NOT NULL,
  status TEXT NOT NULL,
  raw_hash TEXT,
  FOREIGN KEY(account_id) REFERENCES accounts(id)
);

CREATE INDEX idx_quota_account_observed
ON quota_snapshots(account_id, observed_at DESC);
```

---

## 5. Usage Point

```sql
CREATE TABLE usage_points (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL,
  model_id TEXT,
  metric_type TEXT NOT NULL,
  quantity REAL NOT NULL,
  unit TEXT NOT NULL,
  cost REAL,
  currency TEXT,
  period_start TEXT,
  period_end TEXT,
  source TEXT NOT NULL,
  observed_at TEXT NOT NULL,
  FOREIGN KEY(account_id) REFERENCES accounts(id)
);
```

---

## 6. Sync Run

```sql
CREATE TABLE sync_runs (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL,
  started_at TEXT NOT NULL,
  completed_at TEXT,
  status TEXT NOT NULL,
  error_code TEXT,
  retry_after TEXT,
  http_status INTEGER,
  latency_ms INTEGER,
  FOREIGN KEY(account_id) REFERENCES accounts(id)
);
```

不要保存完整响应 body；必要时只保存 hash、错误摘要与结构化解析结果。

---

## 7. Derived Metrics

以下字段不建议持久化，可以实时计算：

```text
usage_percent
remaining_percent
window_elapsed_percent
window_remaining_seconds
freshness_seconds
severity
```

计算：

```text
remaining = limit - used
usage_percent = used / limit * 100
remaining_percent = remaining / limit * 100
```

当 denominator 缺失时返回 `null`，不能强制显示 0%。

---

## 8. Reset 计算

### 已有 reset_at

```text
remaining_seconds = max(reset_at - now, 0)
```

### fixed duration + start

```text
reset_at = start + duration
```

### calendar

根据 timezone 与账单规则计算。

### rolling window

只能在 Provider 明确提供窗口结束时间或可验证规则时计算。

---

## 9. Stale Policy

每个 Provider 有：

```text
fresh_for
stale_after
hard_expire_after
```

示例：

```text
fresh_for          = 10m
stale_after        = 30m
hard_expire_after  = 24h
```

UI：

```text
Fresh  → 正常
Stale  → amber
Expired → gray + Unknown
```

旧数据永远不能伪装成实时数据。

---

## 10. Tauri IPC

### `accounts.list`

返回所有非 secret account metadata。

### `accounts.create`

```ts
{
  providerId: string;
  label: string;
  auth: AuthInput;
}
```

### `accounts.refresh`

```ts
{ accountId: string }
```

### `dashboard.snapshot`

返回当前所有 Provider 的最新标准化 snapshot。

### `provider.test`

测试连接并返回结构化结果。

### `provider.manifest.install`

导入声明式 Provider。

### `settings.update`

只处理非 secret 设置。

---

## 11. Tauri Events

```text
app:ready
provider:sync-start
provider:sync-finish
provider:status-changed
quota:updated
quota:reset-soon
account:auth-required
notification:enqueue
window:expand
window:collapse
```

---

## 12. 前端状态结构

```ts
interface AppState {
  providers: ProviderViewModel[];
  accounts: AccountViewModel[];
  snapshots: Record<string, QuotaSnapshot[]>;
  sync: Record<string, SyncState>;
  ui: {
    bubbleVisible: boolean;
    dashboardOpen: boolean;
    activeAccount?: string;
  };
}
```

不要把 Rust Raw DTO 直接暴露给 UI；使用 ViewModel 做语义转换。

---

## 13. 错误模型

统一错误：

```text
AUTH_REQUIRED
AUTH_EXPIRED
NETWORK_UNAVAILABLE
TIMEOUT
TLS_ERROR
RATE_LIMITED
PROVIDER_UNAVAILABLE
INVALID_RESPONSE
PARSER_FAILED
UNSUPPORTED_CAPABILITY
CONFIG_INVALID
CREDENTIAL_ERROR
STORAGE_ERROR
UNKNOWN
```

错误对象：

```json
{
  "code": "RATE_LIMITED",
  "message": "Provider rate limit reached",
  "retryAt": "2026-10-03T10:24:00Z",
  "retryable": true
}
```

面向用户的 UI 文案不能直接展示内部 stack trace。

---

## 14. 数据保留

建议：

```text
Snapshots       30–90 days
Aggregated daily  1 year
Sync runs        30 days
Raw provider payloads 默认 0
```

让用户可以一键“清除历史数据”。

---
