# 贡献指南

感谢你愿意为 AIQuotaMonitor 贡献力量。以下是与本仓库协作的约定。

## 项目概况

- Windows 桌面应用：Tauri 2 + React/TypeScript + Rust + SQLite。
- 产品定位：多模型平台额度监控中心，不是 Provider 路由或代理工具。
- 领域建模固定为 `Platform → Account → Source → Capability → Snapshot`。
- 平台差异放在 Rust Source adapter，React 页面只消费脱敏 ViewModel。
- 金额使用 Decimal/文本定点值，禁止浮点汇总。
- 有最后成功快照时展示缓存并标记 `stale`；没有真实值时使用 `missing`，禁止补零或伪造。
- 迁移第三方代码时在文件头保留来源仓库、审计提交与 MIT 许可声明。

## 环境准备

```powershell
pnpm install
pnpm --filter @ai-quota-monitor/desktop typecheck   # TS 类型检查
pnpm --filter @ai-quota-monitor/desktop build       # 前端构建
cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml
pnpm --filter @ai-quota-monitor/desktop tauri dev   # 本地启动
```

要求：Windows 10/11、Node.js ≥ 20、pnpm、Rust 1.85+、Tauri 前置依赖（WebView2、Visual Studio C++ Build Tools）。

## 提交流程

1. 从 `main` 拉分支：`git checkout -b feat/xxx`。
2. 小步提交，使用 Conventional Commits 风格：
   - `feat:` 新功能、`fix:` 缺陷修复、`docs:` 文档、`chore:` 杂项、`refactor:` 重构、`perf:` 性能。
   - 示例：`feat: Kimi 控制台消费来源`、`fix: 单 Source 失败不再清空同平台数据`。
3. 提交前运行上方的验证命令，确保 typecheck / build / cargo check 通过。
4. 开 Pull Request，填写 `.github/PULL_REQUEST_TEMPLATE.md` 模板，CI 通过后合并（squash）。

## 代码风格

- 跟随既有代码风格与目录结构：`apps/desktop/src/features/*`、`apps/desktop/src-tauri/src/providers/*`。
- 不引入新依赖或升级框架，除非在 Issue 中说明理由。
- Platform 相关契约变更需同步 `docs/product/platform-access.md`。
- 涉及平台接口时参考 `docs/product/requirements.md` 与 `docs/architecture/`。

## 安全约定

- 前端不接触完整 API Key / Token / Cookie；凭据只进 Windows Credential Manager。
- 不要提交 `.env`、日志、任何真实凭据或测试账号信息。
- 安全问题的报告请走 [SECURITY.md](SECURITY.md) 描述的渠道，不要在公开 Issue 中披露细节。

## 其他

- 提交前检查 `git status` 与 diff；原则上一项任务一个 commit。
- 每次交付说明验证方式、危险边界与回滚方式。
- 有疑问可以先开 Issue 讨论再动手，避免方向性返工。
