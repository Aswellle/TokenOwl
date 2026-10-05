# TokenOwl 开发文档总索引

> **项目定位**：TokenOwl 是一个 Windows / macOS 优先的本地桌面 AI 供应商额度监视器。它以极小的圆形悬浮球常驻桌面，快速显示核心额度状态；点击后展开为完整仪表盘，查看多个 AI 供应商账户的余额、token/credit/request 使用情况、5 小时/7 天/月度等限制窗口、距离下次重置时间、重置进度以及历史使用趋势。
>
> **核心原则**：不把“供应商”写死在 UI；不把“token”作为唯一计量单位；不把网络刷新绑定到 UI 生命周期；不把第三方凭据写入 SQLite；不把不可获得的额度伪装成准确数据。
>
> **文档版本**：1.0
>
> **基准日期**：2026-10-03

## 文档目录

| 文档 | 目的 | AI Agent 是否必读 |
|---|---|---|
| `01-PRD.md` | 产品目标、用户故事、功能规格、版本范围、验收标准 | 是 |
| `02-技术架构.md` | 技术栈、模块、数据流、任务调度、跨平台架构 | 是 |
| `03-Provider接入协议.md` | Provider Adapter / Manifest / 自定义中转站完整设计 | 是 |
| `04-数据模型与内部API.md` | SQLite Schema、领域模型、Tauri IPC、错误模型 | 是 |
| `05-UIUX设计规范.md` | 悬浮球、展开仪表盘、动效、响应式、可访问性 | 是 |
| `06-安全隐私测试与发布.md` | 凭据安全、网络安全、测试、CI/CD、发布 | 是 |
| `07-AI-Agent开发执行规范.md` | 分阶段实施、工作约束、自检、验收、回滚 | **必须优先阅读** |
| `08-开源能力复用与许可证.md` | CodexBar / ccusage / LiteLLM / One API / New API / models.dev 的复用策略 | 是 |

## 推荐实施顺序

```text
Phase 0  工程基线与设计系统
   ↓
Phase 1  Tauri Shell + Floating Bubble + Dashboard Shell
   ↓
Phase 2  Domain Model + SQLite + Keychain + Scheduler
   ↓
Phase 3  Provider SDK/Protocol + Custom OpenAI-Compatible Provider
   ↓
Phase 4  国际首发 Provider
   ↓
Phase 5  中国大陆/亚洲首发 Provider
   ↓
Phase 6  Reset Window + Credit/Grant + History
   ↓
Phase 7  第三方 Manifest Adapter / Provider Registry
   ↓
Phase 8  完整 QA + Performance + Packaging + Release
   ↓
Phase 9  移动端复用 Domain Core
```

## 产品不可妥协项

1. **额度数据永远标记来源、时间和可信度**。
2. **Provider 请求失败不能冻结 UI**；始终使用最近成功快照并明确显示 `stale` 状态。
3. **API Key/OAuth Token/Cookie/Session 不进入普通数据库和日志**。
4. **每一个阶段结束后项目必须可运行、可测试、可打包**。
5. **UI 动画服务于信息理解，不为了炫技增加持续高负载**。
6. **任何 Provider 都不能要求核心 UI 修改代码才能接入**。
7. **中国大陆供应商与国际供应商使用同一领域模型，不做二等适配。**

## 当前已验证的重要开源参考

- CodexBar：<https://github.com/steipete/CodexBar>
- ccusage：<https://github.com/ccusage/ccusage>
- LiteLLM：<https://github.com/BerriAI/litellm>
- One API：<https://github.com/songquanpeng/one-api>
- New API：<https://github.com/QuantumNous/new-api>
- models.dev：<https://github.com/anomalyco/models.dev>
- Tauri 2：<https://v2.tauri.app/>
- Rust keyring：<https://docs.rs/keyring/latest/keyring/>

这些仓库的用途不是要求 TokenOwl 直接整体嵌入，而是用于复用成熟的 Provider 经验、模型/价格数据和本地 usage 解析方法。具体许可证和复制策略见 `08-开源能力复用与许可证.md`。
