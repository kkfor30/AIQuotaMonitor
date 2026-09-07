# Changelog

本文件记录值得注意的变更（Keep a Changelog 风格）。版本号遵循语义化版本。

## [0.1.0] - 2026-09-07

首个正式版本发布（Windows / Tauri 2 + React + Rust）。

### 平台中心与额度监控

- **全平台接入**：支持 GPT/Codex（本机 app-server/WHAM）、Claude Code（CLI OAuth）、Grok（CLI）、Google Antigravity（Prompt Credits & Storage）、DeepSeek、GLM、Kimi、MiMo、MiniMax、SiliconFlow、StepFun、OpenRouter、Novita。
- **多账号与独立会话**：严格遵循 `Platform → Account → Source → Capability → Snapshot` 领域建模，支持多账号分组与独立凭据隔离。
- **真实快照语义**：严格遵循 stale / missing 语义展示，故障隔离不污染，严禁生成虚假补零数据；金额与用量采用文本/定点数精确计算。
- **本地安全优先**：敏感凭据安全存储至 Windows 凭据管理器；业务快照仅持久化于本机 SQLite；前端仅消费脱敏 ViewModel。

### GPT 重置雷达与智能研判

- **双源多渠道聚合**：集成 Codex Radar 官方公开源与 WillCodex 实时源（30分钟级快轮询），并发拉取与智能 ID 去重，时效与稳定性互补保障。
- **全生命周期研判体系**：集成置信度仪表盘（RadarStatusGauge）、决策结论色调流转与参考记录对齐微卡（区分额度卡到账 grant 与额度消耗 drop）。
- **动态回退与本地推测**：空时间窗自动回退至历史有效研判，杜绝误报失败；单条动态支持 AI 逐条快速翻译；明确告知推测属性，不冒充官方结论。

### 桌面常驻悬浮球与高质感交互

- **柔和悬浮小球**：桌面任意边缘停靠吸附与拖拽吸边；搭载克制呼吸微光晕与正圆全息层（杜绝方形边缘裁切）；原生右键快捷菜单（全部刷新、切换详情、隐藏小球、唤起主窗口）。
- **三层流线悬浮详情**：即时呈现核心研判结论、关键参考记录、各平台双栏指标卡片与额度变化趋势，支持单卡刷新与深链直达。
- **主界面体验打磨**：毛玻璃原生视觉，双向缩放控制角标与边框拖拽支持，目录智能检索与编辑吸底布局，内建防白屏容错机制（ErrorBoundary）。

### 合规与工程

- 以 MIT License 发布。
- 迁移代码（DeepSeekMonitorWindows 系列、cc-switch）严格在文件头保留来源仓库、审计提交与 MIT 许可声明。

[0.1.0]: https://github.com/kkfor30/AIQuotaMonitor/releases/tag/v0.1.0

