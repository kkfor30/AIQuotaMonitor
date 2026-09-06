# AIQuotaMonitor 工作规则

## 开始任务前

按顺序阅读：

1. `README.md`
2. `docs/product/requirements.md`
3. `ARCHITECTURE.md`
4. `DECISIONS.md`
5. `_internal/docs/project/roadmap.md`
6. `_internal/docs/project/handoff.md`
7. 与任务相关的 `docs/ui-design/*/README.md`
8. `_internal/docs/iterations.md` 的最新记录
9. 扩平台任务再读 `_internal/docs/project/GLM-ADD-PLATFORMS.md`；国内优先再读 `_internal/docs/project/GLM-ADD-DOMESTIC-PLATFORMS.md`

注：`_internal/` 为不随仓库发布的开发记录，仅在本地存在。

## 产品与数据底线

- 产品核心是多模型平台额度监控，不是 Provider 路由或代理工具。
- 领域层固定按 `Platform → Account → Source → Capability → Snapshot` 建模。
- 一个 Source 失败不能清空同平台其他 Source 的成功数据。
- 有最后成功快照时展示缓存并标记 stale；没有真实值时使用 missing，禁止补零或生成虚假数据。
- 前端只消费脱敏 ViewModel，不直接调用平台接口，不接触完整 API Key、Token 或 Cookie。
- 金额禁止使用浮点数汇总，使用 Decimal/文本定点值。
- GPT 重置雷达只提供推测，不能冒充官方结论；AI 默认可选且不能成为唯一判断来源。

## 工程规则

- 跟随现有 Tauri 2 + React/TypeScript + Rust 结构，不引入独立 HTTP 后端。
- 平台差异放在 Rust Source adapter，不在 React 页面解析原始响应。
- 不复制旧项目的整页 UI、`main.tsx` 或大段 `styles.css`。
- 迁移第三方代码时在文件头保留来源仓库、审计提交与 MIT 许可声明。
- 只读参考仓库：`DeepSeekMonitorWindows-final`（curry880314）与 `cc-switch`（farion1231），不得在本项目任务中修改。
- 不提交 `node_modules`、`dist`、Rust `target`、`_internal/`、IDE 配置、日志或任何凭据。

## 交付与文档

- 每次完成一个可识别阶段，更新 `_internal/docs/project/handoff.md`、`_internal/docs/project/roadmap.md` 和 `_internal/docs/iterations.md`。
- 每次完成一项代码或文档修改后，必须创建一次本地 Git commit；原则上一项任务对应一个 commit，不得夹带其他未完成或无关改动。只有用户明确要求暂不提交时才可例外。
- 提交前必须检查 `git status` 和本次 diff，并完成与风险相称的轻量验证；不得提交尚未完成的试验代码。
- 小改动直接实现；没有明确要求时不增加冗余抽象或升级框架。
- 默认不主动补大量单元测试，但必须执行与改动风险相称的 typecheck、build、cargo check 或手工页面验证。
- 每次交付都要说明：验证方式、危险边界和回滚方式。

## 常用命令

```powershell
pnpm install
pnpm --filter @ai-quota-monitor/desktop typecheck
pnpm --filter @ai-quota-monitor/desktop build
cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml
pnpm --filter @ai-quota-monitor/desktop tauri dev
```
