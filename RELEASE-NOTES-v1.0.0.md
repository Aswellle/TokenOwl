# TokenOwl v1.0.0 — 第一个正式发行版

## 项目简介

TokenOwl 是一款本地优先的 AI 配额监控桌面应用，支持 Windows 和 macOS。它实时追踪多个 AI 提供商的配额、余额和使用情况，帮助用户避免服务中断。

## 核心价值

- **本地优先** — 数据存储在本地 SQLite 和系统钥匙串，不依赖云端服务
- **隐私保护** — API 密钥存储在 OS 钥匙串（Windows Credential Manager / macOS Keychain），绝不写入日志或数据库
- **多提供商** — 支持 8 个 AI 提供商（OpenAI、Anthropic、Gemini、OpenRouter、DeepSeek、Qwen、Kimi、MiniMax）
- **实时监控** — 后台自动刷新配额数据，支持可配置的刷新间隔和智能退避
- **轻量美观** — 浮动气泡 + 可展开仪表盘，支持毛玻璃效果和流畅动画

## 主要功能

### 配额监控
- 实时显示配额使用量、剩余量和限额
- 支持多种配额单位（token、currency、request、credit、point 等）
- 计算配额使用率和压力指数
- 显示配额重置倒计时和历史趋势

### 多账户管理
- 添加、删除和刷新多个 AI 提供商账户
- 支持 OpenAI 兼容 API 的自定义端点
- 每个账户独立显示状态和健康度

### 通知系统
- 配额不足警告（可配置阈值）
- 配额即将重置提醒
- 支持静默模式和仅关键通知模式

### 桌面集成
- 浮动气泡显示整体状态，点击展开仪表盘
- 气泡位置可拖拽并持久化保存
- 系统托盘图标和菜单
- 毛玻璃透明窗口效果（Windows Acrylic / macOS Vibrancy）

### 数据安全
- 端到端使用 HTTPS 与提供商 API 通信
- 自动重试和指数退避策略
- 响应大小限制防止内存耗尽
- 敏感字段自动脱敏（日志和遥测）

## 技术特色

- **Tauri 2** — 轻量级桌面应用框架，打包体积小（Windows MSI ~5.5MB，NSIS ~4MB）
- **Rust 后端** — 高性能、内存安全的系统层逻辑
- **React 19 + TypeScript** — 现代化前端，严格类型检查
- **SQLite 持久化** — 轻量级本地数据库，WAL 模式提升并发性能
- **8 个 Provider 适配器** — 统一的 ProviderAdapter trait，易于扩展新提供商
- **智能调度器** — 基于二叉堆的优先级队列，支持抖动和退避
- **完整 CI/CD** — 前端和 Rust 检查并行运行，自动发布签名安装包

## 系统要求

- **Windows** — Windows 10 或更高版本
- **macOS** — macOS 12 (Monterey) 或更高版本

## 安装

从 [Releases](https://github.com/Aswellle/TokenOwl/releases) 页面下载对应平台的安装包：

- **Windows** — `TokenOwl_1.0.0_x64-setup.nsis.exe`（NSIS 安装器）或 `TokenOwl_1.0.0_x64_en-US.msi`（MSI 安装器）
- **macOS** — `TokenOwl_1.0.0_aarch64.dmg`（Apple Silicon）或 `TokenOwl_1.0.0_x64.dmg`（Intel）

## 使用指南

1. 安装并启动 TokenOwl
2. 点击浮动气泡展开仪表盘
3. 点击"添加账户"，输入 API Key 和 Base URL
4. 配额数据将自动加载并实时更新
5. 拖动气泡到喜欢的位置，位置会自动保存

## 版本规划

- **v1.0.0** — 当前版本，核心配额监控功能
- **v1.1.0** — 多端点数据合并（余额 + 用量 + 配额）
- **v1.2.0** — 自定义通知规则和导出功能
- **v2.0.0** — 插件系统和第三方 provider 扩展

## 开源协议

MIT License — 自由使用、修改和分发。详见 [LICENSE](../LICENSE)。
