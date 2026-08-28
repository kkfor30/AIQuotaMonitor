# 架构概览

## 结论

采用单仓库、单桌面安装包、前后端逻辑分离的 Tauri 架构：React 负责界面，Rust 负责全部敏感数据和平台调用，两者通过 Tauri IPC 通信。MVP 不启动独立 HTTP 后端。

```text
React 主窗口 / 悬浮球窗口
           │ Tauri IPC + Events
           ▼
Commands / ViewModel
           │
           ▼
Refresh Coordinator
├─ Platform Template Registry
├─ Source Adapter Registry
├─ Credential Vault
├─ Snapshot Repository
└─ Status Aggregator
           │
           ├─ SQLite：配置、快照、历史、排序
           ├─ Windows 安全存储：API Key、Token、Cookie
           └─ 官方 API / 网页会话 / 本地 CLI
```

## 核心领域

不能直接沿用 CC Switch 以 CLI Provider 为中心的模型，也不能继续沿用旧项目“一个平台只有一个 QuotaSnapshot”的模型。新版固定使用以下关系：

```text
Platform Template
└─ Account
   ├─ Source: balance_api
   ├─ Source: web_session
   ├─ Source: subscription_oauth
   └─ Source: local_cli / local_usage_log
      └─ Capability Snapshot[]
```

- Platform Template：平台展示信息、支持能力和可用 Source 类型，定义在 Rust 注册表。
- Account：用户在某个平台的一个账户，后续可自然支持多账号。
- Source：一种独立的数据来源、认证和刷新生命周期。
- Capability：余额、消费、窗口额度、Token Plan、缓存命中、Credits、重置信号等。
- Snapshot：某个 Source 在某个时间点获得的真实能力数据。

GPT 重置雷达使用独立边界，不把第三方内容源伪装成模型平台账户：

```text
CodexRadarSource
└─ TiboPostSnapshot[]
   └─ UserConfiguredAiAnalyzer
      └─ RadarAnalysisSnapshot
```

- `CodexRadarSource`：V1 从 Codex Radar 公开页面同步其转载的 Tibo 原文；直接访问 X 是后续可选实现。
- `TiboPostSnapshot`：保存英文原文、发布时间、X 原帖链接、Codex Radar 来源链接、同步时间和 freshness。
- `RadarAnalysisSnapshot`：保存模型标识、输入动态 ID、提示词版本、结论、把握度、引用、正反依据和不确定性。
- Codex Radar 的翻译、信号标签和模型语境解读不得作为用户配置 AI 输入；来源或 AI 失败不改变平台额度状态。

## 前后端边界

### React 前端

- 实现总览、平台中心、GPT 重置雷达、设置和悬浮球。
- 只消费标准化 ViewModel，不解析平台原始响应。
- 使用 TanStack Query 管理 IPC 异步状态，保留上一份可显示数据。
- 不读取、不缓存、不记录完整 API Key、Token 或 Cookie。

### Rust 后端

- 平台模板和 Source 适配器注册。
- 凭据保存、验证、刷新和脱敏。
- 请求去重、超时、有限重试、取消和请求代次控制。
- SQLite schema、迁移、升级前备份和快照历史。
- 聚合平台状态：正常、部分可用、需配置、异常。
- Tauri 窗口创建、边缘吸附、多屏定位和全屏隐藏。

## 存储边界

SQLite 作为非敏感数据的单一事实源，建议首版表：

- `accounts`
- `sources`
- `capability_snapshots`
- `refresh_runs`
- `refresh_results`
- `settings`
- `platform_order`

数据库只保存 `secret_ref`，真实密钥进入 Windows Credential Manager 或 DPAPI 保护的凭据存储。金额使用 Decimal 并以 TEXT 持久化，不能使用浮点数汇总。

## Source 刷新规则

1. 每个 Source 独立刷新，单个失败不取消其他 Source。
2. 网络、超时等瞬时失败不覆盖最后成功快照。
3. 凭据失效、响应结构变化等确定性错误写入结构化状态，但仍保留旧快照供标记为 stale。
4. 没有真实值时返回 missing，禁止补零或生成示例数据。
5. 使用 `(account_id, source_id, generation)` 防止旧请求晚到覆盖新账户或新快照。
6. 后端按 Source 限流；前端重复点击只复用正在执行的刷新任务。

## 未来远程版

如果后续出现团队共享、多设备同步或远程看板需求，再把 Rust 领域层抽取为独立服务。当前先保持 IPC 边界，避免为尚不存在的远程需求增加本机端口、鉴权和部署复杂度。
