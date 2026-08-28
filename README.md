# AIQuotaMonitor

多模型平台统一额度监控中心。目标平台包括 GPT/Codex、Claude Code、DeepSeek、GLM、Kimi、MiMo、MiniMax，并为 GPT 重置雷达保留独立扩展位置。雷达 V1 从 Codex Radar 公开页面同步其转载的 Tibo 原文，再由用户配置的 AI 提供可回链原文的辅助研判；直接访问 X 作为后续可选 Source。

## 项目策略

本项目采用 **同一仓库、前后端逻辑分离、桌面端一体发布** 的方式：

- 前端负责后台看板、悬浮球、平台卡片、主题和用户交互。
- 后端核心负责平台接入、凭据、额度查询、刷新调度、缓存、异常隔离和领域计算。
- 前后端通过明确的数据契约通信，前端不直接调用模型平台，也不保存明文凭据。
- MVP 不拆成两个独立仓库，也不要求用户分别启动前端和后端。

详细说明见 [架构概览](./docs/architecture/overview.md)、[技术选型决策](./docs/architecture/technology-decision.md) 和 [迁移计划](./docs/architecture/migration-plan.md)。

换机继续开发请先阅读 [跨终端交接](./docs/project/handoff.md)，完整需求见 [产品需求](./docs/product/requirements.md)，平台如何接入见 [平台接入需求](./docs/product/platform-access.md)，阶段进度见 [路线图](./docs/project/roadmap.md)。

## 目录

```text
AIQuotaMonitor/
├─ apps/
│  └─ desktop/
│     ├─ src/              # React 后台看板、悬浮球和前端组件
│     └─ src-tauri/src/    # Rust 命令、窗口、存储、刷新和适配器
├─ docs/
│  ├─ architecture/        # 架构决策与数据流
│  └─ ui-design/           # 已确认的 UI 设计稿
├─ tooling/                # 开发、迁移和打包脚本
└─ THIRD_PARTY_NOTICES.md  # 迁移代码来源和许可证记录
```

## 当前状态

- 已完成阶段一桌面骨架与阶段二真实额度数据闭环。
- 已归档平台中心 V4 的三张关键设计稿及前端交接说明。
- 已归档总览、GPT 重置雷达、设置和悬浮球 V5 的六张设计稿及交接说明。
- 已完成 `DeepSeekMonitorWindows-final` 与 `cc-switch` 代码审计。
- 已确定 Tauri 2、React/TypeScript、Rust、SQLite 技术路线。
- 已接入 SQLite v1、Windows Credential Manager、Source 级刷新协调器和真实 ViewModel。
- 平台中心从注册表添加平台；接入表单预填官网链接和官方 API 请求地址。
- DeepSeek 已支持官方余额、网页用量与网页登录；GPT/Codex 已支持本地 app-server 优先、WHAM 回退的窗口额度查询。
- Kimi、GLM 国内/国际、MiniMax 国内/国际已支持官方 Token Plan / Coding Plan 窗口查询。
- 未配置或查询失败时继续使用 missing/stale，不再展示阶段一静态示例额度。

## 本地开发

```powershell
pnpm install
pnpm --filter @ai-quota-monitor/desktop typecheck
pnpm --filter @ai-quota-monitor/desktop build
cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml
pnpm --filter @ai-quota-monitor/desktop tauri dev
```

## 下一步

先完成阶段二真实账号验收，再进入扩展平台：

1. 从平台中心添加 DeepSeek，确认接入表单预填官网和 `https://api.deepseek.com`，填写 API Key 并验证保存。
2. 配置网页会话，确认余额与用量快照。
3. 使用本机 Codex OAuth 登录确认 GPT 5 小时/7 天窗口与可选 Credits。
4. 验证单 Source 失败时其他数据和最后成功快照仍保留。
5. 继续接入 Kimi、GLM、MiniMax、MiMo 与 Claude Code。
