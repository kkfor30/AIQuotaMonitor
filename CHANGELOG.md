# Changelog

本文件记录值得注意的变更（Keep a Changelog 风格）。版本号遵循语义化版本。

## [1.0.0] - 2026-09-08

首个正式版本（Windows / Tauri 2 + React + Rust）。

### 功能

- 平台中心：按注册表添加平台，多账号分组、独立会话；`Platform → Account → Source → Capability → Snapshot` 领域建模。
- 额度监控：GPT/Codex（本机 app-server/WHAM）、Claude Code（CLI OAuth）、Grok（CLI）、DeepSeek、GLM、Kimi、MiMo、MiniMax、SiliconFlow、StepFun、OpenRouter、Novita、Google Antigravity。
- 真实快照语义：stale / missing 展示，不补零、不伪造；金额 Decimal 定点计算。
- 安全存储：凭据进 Windows Credential Manager；数据仅存本机 SQLite；第三方登录由官方 CLI 负责。
- GPT 重置雷达：Codex Radar 公开源 + 本机额度观察 + 可选 AI 分析（默认关闭），仅提供推测。
- 悬浮球：玻璃小球常驻、四级停靠、额度/雷达双级详情；深/浅色主题、自动刷新、开机自启、单实例。
- 设置页：主题、平台排序、刷新计划、开机自启、本机数据目录查看。

### 体验与交互

- **重置雷达**：空时间窗平滑回退历史分析；主卡底部参考记录微卡片；重置卡到账（grant）与消耗（drop）归因区分；置信度仪表与结论胶囊。
- **悬浮球与详情**：呼吸光晕与原生右键快捷菜单（全部刷新、切换详情、隐藏小球、打开主界面）；雷达卡三层流线型卡片；账号卡双栏指标网格；单卡快捷刷新与平台中心深链。
- **主界面**：双向缩放控制角标与边缘拖拽；毛玻璃 Toast、骨架屏、空状态与微动效；平台目录检索与异常呼吸提示；ErrorBoundary 防白屏与滚动穿透优化。

### 合规

- 以 MIT License 发布。
- 迁移代码（DeepSeekMonitorWindows 系列、cc-switch）的来源与许可保留在各迁移文件头注释中。

[1.0.0]: https://github.com/kkfor30/AIQuotaMonitor/releases/tag/v1.0.0
