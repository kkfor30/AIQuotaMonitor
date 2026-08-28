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

GPT 雷达的 AI 默认关闭，只接收 Codex Radar 同步的 Tibo 英文原文和必要的发布时间、原帖链接元数据。AI 是解释器而不是证据来源，每个输出必须引用实际原文；没有真实原文或分析失败时不得生成结论。关闭 AI 后仍可查看来源内容，但状态显示“未分析”。AI 凭据不得进入前端 ViewModel、日志或 SQLite 明文字段。

## D008：迁移代码保留来源

所有实质迁移代码都必须在文件和 `THIRD_PARTY_NOTICES.md` 中记录来源、提交、许可证和改造内容。

## D009：GPT 雷达 V1 使用 Codex Radar 聚合来源

V1 不直接访问 X，使用独立的 `CodexRadarSource` 从 Codex Radar 公开页面同步其转载的 Tibo 原文。只把英文原文、发布时间、X 原帖链接、来源链接和同步元数据作为用户配置 AI 的输入基础；上游翻译、信号标签和模型语境解读不进入用户配置 AI。直接访问 X 保留为后续可替换 Source。雷达领域独立于平台额度领域，不能把第三方页面状态混入 `Platform → Account → Source → Capability → Snapshot` 的平台聚合状态。

## D010：用户从注册表添加平台；默认 API Key，网页登录只补官方缺口

平台目录只显示用户已添加的平台。可添加项来自产品维护的注册表（优先覆盖 cc-switch 已能查询余额/Token Plan 的平台），不开放任意 Base URL。默认接入动作是填写 API Key 并验证。GPT / Claude Code 检测本机 CLI/OAuth。仅当目标字段没有官方接口时才用隔离登录窗，例如 DeepSeek 网页用量与缓存、GLM 个人余额、MiMo 网页会话。不复制 cc-switch 的 Provider 路由、代理或 MCP。
