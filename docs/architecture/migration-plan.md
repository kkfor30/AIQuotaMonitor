# 参考项目迁移计划

## 参考基线

- `DeepSeekMonitorWindows-final`：提交 `f3ab3ec6edfe7670d7a712658fc3ef914c68421d`。
- `cc-switch`：提交 `6243e20ad6f1835f9ac94ab39ea0eb62a6795bc0`，位于 `v3.20.0` 后续主线。

## 迁移矩阵

### 可直接迁移后拆模块

| 来源 | 能力 | 关键文件 | 新位置 |
| --- | --- | --- | --- |
| DeepSeekMonitorWindows | 双悬浮窗、边缘吸附、多显示器、全屏隐藏 | `src-tauri/src/lib.rs` 中 hoverbar commands；`src-tauri/src/hoverbar.rs` | `windows/hoverbar.rs` |
| DeepSeekMonitorWindows | 悬浮球排序与交互状态 | `src/hoverbar-state.ts`、`src/hoverbar-interaction.ts` | `features/hoverbar/` |
| DeepSeekMonitorWindows | DeepSeek 官方余额 | `src-tauri/src/providers/deepseek.rs` | `providers/deepseek/balance.rs` |
| DeepSeekMonitorWindows | Moonshot 官方余额 | `src-tauri/src/providers/moonshot.rs` | `providers/kimi/balance.rs` |
| DeepSeekMonitorWindows | 密钥显示异步防竞态 | `src/credential-secret.ts` | `features/sources/secret-state.ts` |
| cc-switch | SQLite 初始化、版本迁移、升级前备份 | `src-tauri/src/database/` | `storage/`，仅迁移机制不复制业务表 |
| cc-switch | DeepSeek 等官方余额解析 | `src-tauri/src/services/balance.rs` | 对应平台 Source adapter |
| cc-switch | Kimi/GLM/MiniMax Coding Plan | `src-tauri/src/services/coding_plan.rs` | 对应平台 Source adapter |
| cc-switch | Claude/Codex/Gemini 订阅额度 | `src-tauri/src/services/subscription.rs` | `providers/*/subscription.rs` |

### 改造后迁移

- DeepSeek 网页登录、Token 捕获、`amount/cost` 解析：拆为独立 `web_session` Source；增加最后成功快照、部分成功和结构化错误。
- MiMo WebView2 Cookie 登录：保留查询和捕获思路，重做 Cookie 生命周期与独立登录会话协调。
- Codex：保留 app-server 优先、WHAM 回退，但输出改为窗口额度、Credits 和能力级快照。
- Claude Code：保留 `/usage` 解析器；PTY 探测必须改为唯一 session ID、串行锁、超时和定向清理。
- 旧 `quota_cache.rs`：保留凭据指纹和 generation 防旧请求覆盖思路，改为 SQLite 的 Source/Capability 粒度。
- cc-switch 的错误语义：保留“瞬时失败不覆盖旧值、确定性错误明确展示”，改为结构化错误码。
- cc-switch `UsageScript`：MVP 不开放任意脚本；只保留受控内置模板，后续再评估 QuickJS 沙箱。

### 必须重写

- 旧项目 `src/main.tsx` 和 `src/styles.css`。
- 旧项目 `config.json`、`quota-cache-v1.json` 和明文凭据路径。
- 两个项目以 Provider 或硬编码 match 为中心的领域模型。
- 旧项目会把解析失败当作零值的逻辑。
- cc-switch 的代理、协议转换、MCP、Skills、配置接管、云同步和现有 Provider UI。

## 实施顺序

### 阶段 1：可运行骨架

1. 初始化 Tauri、React、TypeScript、pnpm 和 Rust workspace。
2. 建立 `commands/domain/providers/storage/windows` Rust 模块。
3. 建立前端设计 Token、AppShell、四项导航和 Query Client。
4. 迁移悬浮球窗口机制，先使用静态 ViewModel 验证窗口交互。

### 阶段 2：DeepSeek 垂直切片

1. 建立 Account、Source、Capability、Snapshot 和 RefreshRun schema。
2. 迁移 DeepSeek 官方余额适配器。
3. 改造网页登录和消费/模型用量 Source。
4. 实现正常、未配置、部分失败、缓存过期四种 UI 状态。
5. 提供旧 `config.json` 的一次性导入，但迁移后不再明文保存凭据。

### 阶段 3：通用平台能力

1. 迁移 Kimi、GLM、MiniMax、MiMo。
2. 迁移 Codex 和 Claude Code 本地订阅额度。
3. 增加平台模板注册与 capability renderer registry。
4. 完成平台排序、刷新记录和趋势聚合。

### 阶段 4：GPT 重置雷达

1. 实现独立 `CodexRadarSource`，从 Codex Radar 公开页面同步 Tibo 英文原文、发布时间和原帖链接；记录来源健康度、同步时间和结构化解析错误。
2. 按原帖链接与内容哈希去重，单条解析失败不清空其他成功动态；来源失败时保留最后成功快照并标记 stale。
3. 实现用户配置的可选 AI 分析，只发送公开英文原文和必要元数据；上游翻译、信号标签和模型语境解读不进入用户配置 AI。
4. AI 输出保存输入动态 ID、模型标识、提示词版本、结论、把握度、引用、正反依据和不确定性；相同内容、模型和提示词版本复用结果。
5. 在信号摘要中独立展示 Codex Radar 内容、AI 辅助结论和已接入的 Codex 额度上下文。该能力独立于平台额度 Source，不反向污染通用平台模型。
6. 直接访问 X 保留为后续可替换 Source，V1 不实现关键词规则、权重、回测或规则版本。

## 迁移验收门槛

- 每个迁移文件记录原仓库和提交号。
- 解析器使用真实响应夹具覆盖字段缺失和格式变化。
- 凭据、Cookie 和 Token 不进入前端 DTO、日志或 SQLite 明文字段。
- 一个 Source 失败时，其他 Source 和最后成功快照仍可用。
- 金额不使用 f64；缺失值不显示为零。
- 外部请求具备明确超时、有限重试和平台级限流。
