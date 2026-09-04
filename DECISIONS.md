# 关键决策

## D001：Tauri 内嵌 Rust 后端

代码层面前后端分离，部署层面保持一个桌面应用。React 通过 Tauri IPC 调用 Rust，不开放本机 HTTP 端口。

## D002：新项目重建，不 fork 参考项目

`DeepSeekMonitorWindows-final` 负责提供成熟 Windows 悬浮窗和平台查询实现；`cc-switch` 负责提供额度查询、SQLite 迁移和错误语义参考。两者领域模型和现有 UI 都不直接继承。

## D003：Source 级领域模型

统一模型为 `Platform → Account → Source → Capability → Snapshot`。平台聚合状态由 Source 计算，支持“余额正常、网页登录失败、旧用量缓存仍可见”。

## D004：SQLite 不存明文秘密

SQLite 存配置、`secret_ref`、快照、刷新历史和排序；API Key、Token、Cookie 使用 Windows Credential Manager 或 DPAPI 保护。旧项目明文 `config.json` 只能作为一次性导入来源。

## D005：四项一级导航

固定为“总览、平台中心、GPT 重置雷达、设置”。模型额度与账户接入合并进平台中心；Codex Radar 内容源与 AI 辅助分析归入雷达；悬浮球配置合并进设置。

## D006：前端按能力渲染

平台中心保留统一骨架，通过 capability renderer 展示余额、Token Plan、窗口额度、消费、缓存命中或 Credits，禁止每增加一个平台就复制页面。

## D007：AI 分析必须可选

GPT 雷达的 AI 默认关闭，只接收 Codex Radar 同步的 Tibo 英文原文和必要的发布时间、原帖链接元数据。用户可另存一份语义提示（默认区分重置卡/额度重置/无信号，并含仪表盘/里程碑等措辞），作为分析时的额外说明，但不能替代原文，也不得把凭据或上游中文解读塞进模型输入。固定系统提示才是 banked_reset / quota_reset 的权威定义；用户提示只能补充 Tibo 用语。AI 是解释器而不是证据来源，每个输出必须引用实际原文；没有真实原文或分析失败时不得生成结论。AI 开关只控制是否运行新分析：关闭时仍同步来源并展示来源标签，不得显示“暂无信号”；历史分析保留并标注时间。AI 凭据不得进入前端 ViewModel、日志或 SQLite 明文字段。

## D008：迁移代码保留来源

所有实质迁移代码都必须在文件和 `THIRD_PARTY_NOTICES.md` 中记录来源、提交、许可证和改造内容。

## D009：GPT 雷达 V1 使用 Codex Radar 聚合来源

V1 不直接访问 X，使用独立的 `CodexRadarSource` 从 `https://codexradar.com/` 公开首页同步其转载的 Tibo 原文、中文翻译、信号标签和模型语境解读。只把英文原文、发布时间、X 原帖链接作为用户配置 AI 的输入基础；上游翻译、信号标签和模型语境解读不进入用户配置 AI。直接访问 X 保留为后续可替换 Source。雷达领域独立于平台额度领域，不能把第三方页面状态混入 `Platform → Account → Source → Capability → Snapshot` 的平台聚合状态。

## D010：用户从注册表添加平台；默认 API Key，网页登录只补官方缺口

平台目录只显示用户已添加的平台。可添加项来自产品维护的注册表（优先覆盖 cc-switch 已能查询余额/Token Plan 的平台），不开放注册表以外的供应商。接入表单必须展示官网链接和官方 API 请求地址（预填完整 URL），让用户看见额度查询打到哪里。默认动作是填写 API Key 并验证。GPT 默认检测本机 Codex 登录，不必先开网页；额外 ChatGPT 账号用独立 Codex 目录登录，不覆盖 `~/.codex`。Claude Code 检测本机 CLI。仅当目标字段没有官方接口时才用隔离登录窗，例如 DeepSeek 网页用量与缓存、GLM 个人余额、MiMo 网页会话。不复制 cc-switch 的 Provider 路由、测速代理或 MCP。

## D011：时间解析与事件状态确定性优先

帖内时间声明（6pm PST 等）由 Rust `time_claims`（time-v1）规则解析为北京时间，AI 只做语义判断不做算术；无法确定日期/时区/am pm 时降级 ambiguous，禁止输出精确北京时间。事件状态推进与关闭由确定性代码完成（`reconcile_event_state`：额度观察推进 landed_observed、expected 过期仅改文案、超时关闭、historical_replay 不改事件）；分析结论带 temporal_phase/valid_until 时效字段，事件状态变更使缓存键失效。额度重置观察固化为 `quota_reset_observations` 记录，observation 上的用户确认只改重置卡归因，不推进事件。

## D012：雷达生命周期消费与用户确认重置

帖子用 `lifecycle_consumed_at` 标记是否已成功参与实时分析。用户时间范围只控制展示和 AI 背景，不得用来判断“是否新增”。只有 NEW POSTS 可以创建或推进事件；EVENT CONTEXT / HISTORICAL CONTEXT 不能作为新事件的唯一依据。事件证据只收录被 citations 引用的新增帖和必要事件上下文，不得把整批时间窗帖子写入 `radar_event_evidence`。`radar_events.user_confirmed_reset_at` 记录“用户确认额度已重置/重置卡已到账”，进入 24 小时观察期，可撤销；本机 `observed_reset_at` 优先于用户确认。提示词版本 radar-v18。

## D013：最近一次重置只认本机观察或用户确认

“最近一次重置”唯一来源是 `latest_confirmed_reset_event`：`closed_at IS NOT NULL AND event_type = 'quota_reset' AND (observed_reset_at OR user_confirmed_reset_at)`，按 `COALESCE(observed_reset_at, user_confirmed_reset_at)` 倒序。重置卡到账不是额度重置，不得写入该摘要。invalid_historical_replay、timeout、claimed_unverified 等普通关闭事件只能进 `recentClosedEvent`（历史/来源声称提示），禁止用 claimed_landed_at 或 closed_at 生成“最近一次重置”；仅有来源声称时文案为“最近一次来源声称于 …·尚未验证”。possible_reset 只是疑似额度刷新，不更新“最近一次重置”。分析记录持久化 new/event_context/historical 三组 post id（SQLite v11）和 signal_type（SQLite v12），“当前判断依据”引用限定为当前分析 citations ∩ newPostIds，历史引用不进入当前依据。CodexRadar 公告持久化：解析为空时保留最后一次公告并标记非当前。

## D014：重置卡与额度重置双轨

AI 输出 `signal_type`：`banked_reset`（可保存重置卡发放/到账）、`quota_reset`（额度窗口实际刷新）、`none`（无信号）。事件表 `event_type` 同步为 banked_reset / quota_reset；旧事件迁移为 quota_reset，旧分析标 unknown，不得伪造成新分析。两类活动事件不得互相关闭。额度窗口观察只推进 quota_reset；banked_reset_count 增加才确认“本机观察到重置卡到账”，数量减少不能单独断言已使用。Codex app-server / WHAM 的 `rateLimitResetCredits.availableCount`（兼容 snake_case）解析为 capability `banked_reset_count`：真实 0 必须保存，字段缺失保持 missing，禁止补零，且不得与 credits.balance 换算。本机默认账号与额外 GPT 账号走同一解析器，按各自 Source 独立保存。关键词只出现在 AI 语义提示，不引入规则引擎。
