# AIQuotaMonitor

> 多模型平台统一额度监控中心 —— 常驻 Windows 托盘的桌面监控器，统一查看各模型平台的额度、订阅窗口与消费趋势，并内置「GPT 重置雷达」防止错过额度重置窗口。

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Tauri 2](https://img.shields.io/badge/Tauri-2-24C8DB)](https://tauri.app)
![Platform](https://img.shields.io/badge/Platform-Windows%2010%2F11-0078D4)

## 功能

- **平台中心**：从注册表平台中按需添加（DeepSeek、GLM、Kimi、MiMo、MiniMax、SiliconFlow、StepFun、OpenRouter、Novita），API Key 填写后先验证连接再保存；平台、来源、账号按 `Platform → Account → Source → Capability → Snapshot` 领域模型组织。
- **本地订阅检测**：GPT/Codex（app-server 优先、WHAM 回退）、Claude Code、Grok CLI 自动检测本机登录；GPT 额外账号由应用调用官方 Codex 组件拉起浏览器 OAuth，并保存到独立目录，不覆盖本机账号。本应用不代管第三方登录。
- **多账号**：同平台多账号分组展示、重命名、独立会话，异常隔离到 Source 级——单个来源失败不影响其他来源的成功快照。
- **真实快照语义**：全部数据为真实快照；失败时保留最后成功快照并标记 `stale`，无真实值时为 `missing`，从不补零或伪造数据；金额全部使用定点 Decimal 计算。
- **安全存储**：API Key 等凭据存入 Windows Credential Manager，SQLite 只保存凭据引用；数据库存储本机数据，应用不向任何服务器上传数据。
- **悬浮球**：五边形玻璃小球常驻屏幕，悬停查看额度摘要、四级停靠（顶/底/左/右）、刷新与确认操作。
- **GPT 重置雷达**：同步 CodexRadar 与 WillCodex 公开页面（分来源记录成功与失败状态），应用运行期间每 15 分钟自动检查，三路证据（来源声称 / 本机额度观察 / 可选 AI 辅助分析）综合研判重置卡与额度重置信号，事件按「已观察落地 / 用户确认 / 已撤回 / 超时未验证」归档历史；**雷达只提供推测，不冒充官方结论**，AI 分析默认关闭、始终可替代。
- **设置**：主题（浅色 / 深色）、平台排序、自动刷新、开机自启、本机数据目录查看。

## 平台支持

| 平台 | 额度来源 | 展示能力 |
| --- | --- | --- |
| GPT / Codex | 本机 app-server / WHAM | 5 小时、7 天窗口，额外余额，可用重置卡 |
| Claude Code | 本机 CLI OAuth | 5 小时 / 7 天 / Opus / Sonnet 窗口 |
| Grok | 本机 CLI | SuperGrok 7 天窗口 |
| DeepSeek | 官方余额 API + 网页用量 | 余额、消费、模型 Token、缓存命中率、消费趋势 |
| GLM | 官方 Coding/Token Plan + 网页个人余额 | 窗口额度、剩余百分比 |
| Kimi | 官方 Coding Plan + Moonshot 余额 | 窗口额度、余额 |
| MiniMax | 官方 Coding/Token Plan（国内/国际） | 窗口额度 |
| MiMo | 网页会话余额 | 余额 |
| SiliconFlow / StepFun / OpenRouter / Novita | 官方余额 API | 余额、累计消费（OpenRouter） |

各平台仅调用官方公开接口或本机 CLI/官方网页登录会话；网页登录在隔离窗口内完成，登录窗口不授予应用 IPC。

## 界面

**总览** —— 所有平台的门户：额度卡片、消费趋势与最近刷新记录。

![总览](docs/screenshots/overview.png)

**平台中心** —— 单平台深度视图：账户窗口额度、资金余额、模型用量与缓存效率。

![平台中心](docs/screenshots/platform-center.png)

**GPT 重置雷达** —— 重置判断中心：Codex Radar 来源、本机额度观察与可选 AI 分析三路证据。

![GPT 重置雷达](docs/screenshots/radar-signal.png)

**悬浮球** —— 常驻屏幕边缘的即时额度速览，悬停展开详情：

<table>
  <tr>
    <td align="center"><img src="docs/screenshots/hoverbar.png" alt="悬浮球额度卡" width="48%"><br><sub>额度卡：窗口进度、可用重置卡、额外余额</sub></td>
    <td align="center"><img src="docs/screenshots/hoverbar-glm.png" alt="悬浮球 GLM 卡" width="48%"><br><sub>多平台卡片：额度与雷达状态速览</sub></td>
  </tr>
  <tr>
    <td align="center"><img src="docs/screenshots/hoverbar-radar.png" alt="悬浮球雷达详情" width="48%"><br><sub>雷达详情：来源动态、最近一次重置</sub></td>
    <td align="center"><img src="docs/screenshots/hoverbar-radar-ai.png" alt="悬浮球雷达 AI 分析" width="48%"><br><sub>AI 分析：结论、依据与原帖引用</sub></td>
  </tr>
</table>

## 安装与运行

从 [Releases](../../releases/latest) 下载 Windows 安装包（NSIS），或从源码构建：

```powershell
pnpm install
pnpm package            # 构建发布包，产物在 src-tauri/target/release/bundle/
```

本地开发：

```powershell
pnpm install
pnpm --filter @ai-quota-monitor/desktop typecheck
pnpm --filter @ai-quota-monitor/desktop build
cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml
pnpm dev                # tauri dev，热更新
```

前置要求：Windows 10/11、Node.js ≥ 20、pnpm、Rust 1.85+、[Tauri 前置依赖](https://tauri.app/start/prerequisites/)（WebView2、Visual Studio C++ Build Tools）。

## 目录

```text
AIQuotaMonitor/
├─ apps/
│  └─ desktop/
│     ├─ src/              # React 后台看板、悬浮球和前端组件
│     └─ src-tauri/src/    # Rust 命令、窗口、存储、刷新和平台适配器
├─ docs/
│  ├─ architecture/        # 架构与决策
│  ├─ product/             # 产品需求与平台接入说明
│  └─ ui-design/           # 设计稿归档
├─ tooling/                # 开发、迁移和打包脚本
├─ LICENSE
└─ docs/                   # 各迁移文件头保留来源与许可声明
```

详细说明见 [架构概览](docs/architecture/overview.md)、[技术选型](docs/architecture/technology-decision.md)、[产品需求](docs/product/requirements.md) 与 [平台接入说明](docs/product/platform-access.md)。

## 安全与边界

- **凭据**：仅存入 Windows Credential Manager，代码中不存在明文密钥；应用不向外部服务上传任何数据（雷达同步仅抓取公开页面）。
- **平台接口**：部分能力（GLM、MiMo 网页余额、Codex WHAM 等）依赖平台内部接口或公开页面，可能随平台调整失效；应用对结构变化回报 `structure_changed` 错误并保留最后成功快照，不会崩溃或清空数据。
- **平台商标**：`apps/desktop/public/assets/providers/` 下各平台 Logo 仅用于识别对应平台，版权与商标归各平台所有者。
- **雷达推测**：GPT 重置雷达的判断基于来源、本机观察与可选 AI，可能误判；展示层明确标注「仅为推测」。

发现安全漏洞请通过 GitHub Issue（限定 `Security` 标签）或直接私信仓库维护者报告敏感信息，不要公开描述可利用细节。

## 许可证

MIT License，详见 [LICENSE](LICENSE)。

## 参与贡献

- 提交 Bug / 功能建议：请使用 [Issue 模板](.github/ISSUE_TEMPLATE/bug_report.md)，安全问题请走 [SECURITY.md](SECURITY.md) 私密渠道；
- 提交代码：阅读 [CONTRIBUTING.md](CONTRIBUTING.md)；
- 行为准则：[CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)；
- 版本变更：[CHANGELOG.md](CHANGELOG.md)。

