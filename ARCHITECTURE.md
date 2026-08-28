# Architecture

详细架构见：

- `docs/architecture/overview.md`
- `docs/architecture/technology-decision.md`
- `docs/architecture/migration-plan.md`

## 不可破坏的边界

```text
React 主窗口 / 悬浮球
        │ Tauri IPC + Events
        ▼
Commands / ViewModel
        ▼
Refresh Coordinator
├─ Platform Template Registry
├─ Source Adapter Registry
├─ Credential Vault
├─ Snapshot Repository
└─ Status Aggregator
        ├─ SQLite：非敏感配置、快照、历史、排序
        ├─ Windows 安全存储：API Key、Token、Cookie
        └─ API / Web Session / Local CLI
```

- MVP 是一个 Tauri 安装包，不启动独立本机 HTTP 服务。
- React 不解析平台原始响应，不保存秘密。
- Source 独立认证、独立刷新、独立缓存和独立错误状态。
- 主窗口与悬浮球共享同一份后端 ViewModel 和前端 Query Cache。
- 平台注册表描述能力；用户选择添加哪些平台后，页面根据 capability renderer 渲染，禁止为每个平台复制整页。
- GPT 重置雷达是独立领域：`CodexRadarSource → TiboPostSnapshot → UserConfiguredAiAnalyzer → RadarAnalysisSnapshot`。V1 不直接访问 X；雷达来源失败不污染平台额度聚合状态。

## 当前代码布局

```text
apps/desktop/src/
├─ app/                # 应用入口与一级页面状态
├─ components/         # 通用布局和 UI 原语
├─ features/           # overview/platform-center/radar/settings/hoverbar
├─ lib/                # IPC、Query Client、类型、格式化
└─ styles/             # 设计 Token 与全局样式

apps/desktop/src-tauri/src/
├─ commands/           # Tauri IPC 命令
├─ domain/             # 脱敏 ViewModel 与领域类型
├─ providers/          # DeepSeek、GPT/Codex、Kimi、GLM、MiMo、MiniMax Source adapters 与平台模板
├─ refresh/            # Source 并行刷新、去重与 generation 协调
├─ storage/            # SQLite、Windows Credential Manager、旧配置导入与窗口偏好
└─ windows/            # 悬浮球、隔离登录窗（DeepSeek / GLM / MiMo）与桌面窗口能力
```
