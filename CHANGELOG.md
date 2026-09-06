# Changelog

本文件记录值得注意的变更（Keep a Changelog 风格）。版本号遵循语义化版本。

## [0.1.0] - 2026-09-06

首个公开版本发布（Windows / Tauri 2 + React + Rust）。

### 功能

- 平台中心：按注册表添加平台，多账号分组、独立会话；`Platform → Account → Source → Capability → Snapshot` 领域建模。
- 额度监控：GPT/Codex（本机 app-server/WHAM）、Claude Code（CLI OAuth）、Grok（CLI）、DeepSeek、GLM、Kimi、MiMo、MiniMax、SiliconFlow、StepFun、OpenRouter、Novita。
- 真实快照语义：stale / missing 展示，不补零、不伪造；金额 Decimal 定点计算。
- 安全存储：凭据进 Windows Credential Manager；数据仅存本机 SQLite；第三方登录由官方 CLI 负责。
- GPT 重置雷达：Codex Radar 公开源 + 本机额度观察 + 可选 AI 分析（默认关闭），仅提供推测。
- 悬浮球：玻璃小球常驻、四级停靠、额度/雷达双级详情；深/浅色主题、自动刷新、开机自启、单实例。
- 设置页：主题、平台排序、刷新计划、开机自启、本机数据目录查看。

### 合规

- 以 MIT License 发布。
- 迁移代码（DeepSeekMonitorWindows 系列、cc-switch）的来源与许可保留在各迁移文件头注释中。

[0.1.0]: https://github.com/kkfor30/AIQuotaMonitor/releases/tag/v0.1.0
