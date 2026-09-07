# Changelog

本文件记录值得注意的变更（Keep a Changelog 风格）。版本号遵循语义化版本。

## [0.1.1] - 2026-09-07

### 功能与优化

- **新增平台监控**：接入 Google Antigravity 本机双轨额度（Prompt Credits & Storage）监控与独立会话管理。
- **GPT 重置雷达体验重构**：
  - 修复空时间窗状态语义：当窗内无新动态时平滑回退至“历史分析”，杜绝误报“AI 分析失败”；
  - 重构主卡底部参考记录：采用对齐前置图标微卡片，明确呈现“上次重置卡到账”与“最近一次重置”；
  - 细分重置卡归因变化：区分额度卡到账（grant）与数量消耗（drop），完善本机观察记录；
  - 首屏置信度仪表与研判结论胶囊升级。
- **悬浮球与详情页深度优化**：
  - 悬浮小球引入呼吸光晕与原生右键快捷菜单（一键全部刷新、切换详情、隐藏小球、打开主界面）；
  - 悬浮条雷达卡重构为三层流线型卡片（核心研判、参考记录、时效与微型 Pill 徽章）；
  - 悬浮详情账号卡片升级为双栏指标网格（额度变化、重置卡与权益），移除粗绿悬空竖条，改用状态边框；
  - 支持单卡快捷刷新与直达平台中心深链。
- **主界面与无边框交互升级**：
  - 主窗口提供双向缩放控制角标与边缘拖拽支持（Resize Handles）；
  - 全局毛玻璃 Toast 反馈体系、骨架屏、空状态与微动效打磨；
  - 平台目录支持快速检索、异常状态呼吸提示与编辑抽屉吸底布局；
  - 防白屏容错边界（ErrorBoundary）与滚动穿透优化。

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

[0.1.1]: https://github.com/kkfor30/AIQuotaMonitor/releases/tag/v0.1.1
[0.1.0]: https://github.com/kkfor30/AIQuotaMonitor/releases/tag/v0.1.0
