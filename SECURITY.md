# 安全策略

AIQuotaMonitor 在 Windows 上管理多平台 AI 额度凭据，属于安全敏感应用。本文件描述安全边界与报告渠道。

## 安全边界（已实现）

- **凭据存储**：API Key、Token、Session Cookie 等仅写入 Windows Credential Manager，SQLite 只保存凭据引用；代码中不存在明文密钥。
- **本机数据**：额度快照只存本机 SQLite；应用不向任何服务器上传用户数据（GPT 重置雷达仅同步 Codex Radar 公开页面）。
- **第三方登录隔离**：网页登录在隔离 WebView 窗口完成，不授予 Tauri IPC；应用只读本机 CLI 登录（Codex/Grok/Claude），登录与续期由官方 CLI 负责。
- **前端脱敏**：React 层只消费脱敏 ViewModel，不接触完整凭据。

## 潜在风险点（维护者已知）

- 平台内部接口（部分网页余额、WHAM 等）可能随平台调整失效。
- Windows 凭据缓存、WebView2 会话目录中暂存的凭据由系统管理，卸载应用不会主动清除所有残留。

## 报告漏洞

通过 **GitHub 私密漏洞报告**（Settings → Security → Private vulnerability reporting，已开启）提交，或发送邮件到维护者（见仓库 GitHub 账号）。

**报告时包含**：

- 影响的版本或提交；
- 完整复现步骤与最小示例；
- 影响的严重程度（是否可远程利用、是否暴露他人凭据）；
- 你希望如何署名（可选）。

**请勿**在公开 Issue、Pull Request 中发现或贴出漏洞细节；报告后我会尽快响应。作为非商业项目，可能无法提供外部安全奖励。

## 开发者注意

- 禁止提交 `.env`、日志、测试账号、真实凭据或截图中的未脱敏账号名；
- 涉及凭据、Cookie、登录的改动必须经过人工审查，勿降低既有隔离等级；
- 发现本仓库历史中混入的凭据，请通过上述渠道报告，不要公开评论。
