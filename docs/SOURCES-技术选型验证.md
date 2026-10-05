# TokenOwl 技术选型验证来源

> 仅记录本方案中依赖的外部项目/平台事实。供应商 API 变化很快，真正实现某 Provider 前仍需重新验证官方文档。
>
> 验证日期：2026-10-03

## Tauri 2

- https://tauri.app/start/create-project/
- https://v2.tauri.app/zh-cn/start/migrate/from-tauri-1/

Tauri 官方当前提供 React/TypeScript 模板，并有移动端共享 Rust library 的架构要求。

## CodexBar

- https://github.com/steipete/CodexBar

当前仓库页面显示 MIT，并列出 OpenAI、Anthropic、Alibaba Coding Plan、Alibaba Token Plan、Qwen Cloud、Kimi、MiniMax、Doubao、DeepSeek、Moonshot、Xiaomi MiMo 等 Provider。

## ccusage

- https://github.com/ccusage/ccusage

当前仓库支持 daily/weekly/monthly/session 与 Claude Code 5-hour blocks，并覆盖多个 CLI usage source。

## LiteLLM

- https://github.com/BerriAI/litellm

当前仓库 README 描述统一调用 100+ LLM providers，并支持 cost tracking 等能力。

## One API

- https://github.com/songquanpeng/one-api

当前 README 列出 DeepSeek、豆包、文心、通义千问、星火、智谱、腾讯混元、Moonshot、百川、MiniMax、零一万物、阶跃星辰、SiliconFlow 等国内/国际 Provider。

## New API

- https://github.com/QuantumNous/new-api

当前仓库页面显示 AGPL-3.0，并以统一 AI model hub / gateway 为定位。

## models.dev

- https://github.com/anomalyco/models.dev

当前仓库提供 model specifications、pricing、capabilities、provider data，并提供 API。

## Rust keyring

- https://docs.rs/keyring/latest/keyring/
- https://docs.rs/keyring/latest/keyring/v1/

当前文档说明其 v1 接口可跨平台访问 macOS Keychain、Windows Credential Manager 等凭据存储。
