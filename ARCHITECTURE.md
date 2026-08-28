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
- 平台模板描述能力；页面根据 capability renderer 渲染，禁止为每个平台复制整页。
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
├─ providers/          # 阶段一静态数据；阶段二替换为 Source adapters
├─ storage/            # 当前悬浮球偏好；阶段二加入 SQLite/Vault
└─ windows/            # 悬浮球与桌面窗口能力
```
