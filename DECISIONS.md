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

固定为“总览、平台中心、GPT 重置雷达、设置”。模型额度与账户接入合并进平台中心；规则与 AI 合并进雷达；悬浮球配置合并进设置。

## D006：前端按能力渲染

平台中心保留统一骨架，通过 capability renderer 展示余额、Token Plan、窗口额度、消费、缓存命中或 Credits，禁止每增加一个平台就复制页面。

## D007：AI 分析必须可选

GPT 雷达的 AI 默认关闭，只接收公开文本和脱敏摘要，并设置对最终结论的影响上限。规则和公开来源必须能够在无 AI 时独立工作。

## D008：迁移代码保留来源

所有实质迁移代码都必须在文件和 `THIRD_PARTY_NOTICES.md` 中记录来源、提交、许可证和改造内容。
