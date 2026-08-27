# 技术选型决策

## 决策

| 层级 | 选择 | 依据 |
| --- | --- | --- |
| 桌面壳 | Tauri 2 | 旧项目的 Windows 悬浮球、托盘、透明窗口和 WebView2 登录已验证 |
| 前端 | React 18 + TypeScript | 两个参考项目均使用，旧逻辑和 cc-switch 查询 UI 可低成本迁移 |
| 构建 | Vite 7 + pnpm | cc-switch 当前主线已验证，适合新前端重建 |
| Rust | Edition 2021，最低 1.85 | 兼容 cc-switch 查询与 SQLite 依赖；当前机器已安装 1.96 |
| IPC | Tauri Commands + Events | 不暴露本机 HTTP 端口，适合单机桌面应用 |
| 数据库 | SQLite + rusqlite bundled | 支持事务、迁移、历史查询、备份和单文件分发 |
| 网络 | Tokio + reqwest 0.12 + rustls | 复用两个项目已有异步查询代码并统一超时 |
| 金额 | rust_decimal，数据库 TEXT | 避免旧代码 f64 汇总误差 |
| 请求状态 | TanStack Query | 借鉴 cc-switch 的上一成功值保留和缓存同步 |
| UI 基础 | Tailwind + CSS 变量 + Radix primitives | 只承担布局、Token 和无样式交互原语；视觉按新版设计重做 |
| 图表/排序 | Recharts + dnd-kit | cc-switch 已验证；对应趋势图与用户自定义排序 |

## 不采用独立后端进程

MVP 的“后端”就是 Tauri 内的 Rust 层。它与 React 代码分离，但随同一个桌面程序启动和发布。这样可以直接复用 Windows 窗口能力、避免本机端口和 CORS，同时减少安装与升级复杂度。

## 不直接 fork 任一参考项目

- `DeepSeekMonitorWindows-final` 的桌面能力成熟，但 `main.tsx`、`styles.css`、`lib.rs` 过度集中，JSON 存储和平台硬编码不能继续扩展。
- `cc-switch` 的查询与 SQLite 机制成熟，但其领域是 CLI Provider 切换、代理和配置接管，不是多 Source 额度监控。
- 新项目采用干净骨架，只迁移有明确边界的模块，并记录来源提交。

## 前端实现原则

- 不迁移两边现有页面和大段样式。
- 按新版 UI 拆 `AppShell`、`ProviderRail`、`UsageView`、`SourcesView` 和 `SourceEditorDrawer`。
- 平台差异通过后端返回的 capability 模板与前端 renderer registry 处理，禁止复制整页。
- Tailwind/Radix 不是视觉模板；颜色、玻璃层级、间距、字体和动效统一由项目设计 Token 控制。
- 主窗口和悬浮窗口共享 Query 数据与领域状态，不各自重复请求平台接口。

## 版本策略

初始化时锁定一组可构建版本，不在迁移中同时追求升级。参考基线：

- Tauri `2.11.x`
- React `18.3.x`
- TypeScript `5.9.x`
- Vite `7.3.x`
- rusqlite `0.31.x`
- reqwest `0.12.x`
- TanStack Query `5.90.x`

实际初始化后以 lockfile 为准，后续升级单独进行。
