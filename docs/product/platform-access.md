# 平台接入需求

本应用是额度监控中心，不是 Provider 路由或 API 代理。用户从**产品提供的平台注册表**里选择要监控哪些平台，而不是启动后默认铺满全部平台，也不是任意粘贴中转 Base URL。

## 1. 产品要改成什么样

当前实现把 DeepSeek、GPT、Claude、GLM、Kimi、MiMo、MiniMax 全部写死在平台目录里。未接入的项没有 Source 卡片，看起来像坏了。

正确交互：

1. 平台目录只显示用户已经添加的平台。
2. 用户点「添加平台」，从注册表勾选要监控的平台。
3. 添加后进入该平台的 `接入与来源`，按 Source 完成凭据。
4. 大多数平台填 API Key、保存前验证即可。
5. 个别能力没有官方接口时，再用网页登录抓字段。
6. GPT / Claude Code 这类本机 CLI 订阅，检测本机登录，不在本应用再做网页登录。

移除已添加的平台需要二次确认，并说明会失去该平台快照和凭据引用。

## 2. 两条接入路径

每个 Source 只走一条。前端只渲染脱敏 ViewModel。

### A. 官方查询（默认，填 API Key 或检测本地登录）

有官方余额、Token Plan 或本机 CLI/OAuth 可读额度时用这条路径。实现参考只读仓库 `D:\AIproject\cc-switch`：

| 能力 | cc-switch 文件 |
| --- | --- |
| DeepSeek、硅基流动、OpenRouter 等官方余额 | `src-tauri/src/services/balance.rs` |
| Kimi / GLM / MiniMax Token Plan | `src-tauri/src/services/coding_plan.rs` |
| Codex / Claude / Gemini 本地订阅 | `src-tauri/src/services/subscription.rs` |

用户侧固定四步，对齐 cc-switch 的添加供应商习惯，但只用于额度监控：

1. 点「添加平台」，从注册表选择平台。
2. 立刻展示该平台表单：供应商名称、备注、官网链接、API Key、「获取 API Key」、官方 API 请求地址（完整 URL）。请求地址预填注册表中的官方端点，必须可见，不能藏成内部常量。
3. 点「验证连接」，用填写的 API Key 和请求地址调用官方接口，成功才解锁保存。
4. 用户再点「保存」，凭据写入 Credential Manager 并刷新。

「管理与测速」打开端点测速面板：对比额度查询地址延迟，选中后写回 API 请求地址。本应用不做请求代理。不要把「验证」和「保存」合成一个按钮。验证失败不能保存。

金额用 Decimal/文本定点。凭据进 Windows Credential Manager。失败时保留最后成功快照。

### B. 会话抓取（仅无官方接口的字段）

官方没有对应额度、余额、用量或缓存字段时才用隔离登录窗。

已知需要这条路径的能力：

- DeepSeek 模型 Token、请求数、缓存命中、消费趋势
- GLM 个人账户余额（官方 Coding Plan 不覆盖时）
- MiMo 网页会话余额（没有官方余额接口时）

规则：登录窗没有主窗口 Tauri IPC；验证成功再写入凭据；登录中途空用量不得覆盖上次成功快照；不开放任意用量脚本。

## 3. 可添加的平台注册表

注册表由产品维护。用户只能添加表中的平台，不能自定义未知供应商。同一平台可有多个 Source。

### 填 API Key 即可（优先按 cc-switch 接入）

| 平台 | 查询内容 | cc-switch 依据 | 状态 |
| --- | --- | --- | --- |
| DeepSeek | 账户余额 | `balance.rs` → `api.deepseek.com/user/balance` | ✅ 已实现 |
| Kimi | Coding Plan 窗口 | `coding_plan.rs` → `api.kimi.com/coding` | ✅ 已实现 |
| Kimi | 个人账户余额 | 官方 `api.moonshot.cn/v1/users/me/balance`，使用开放平台 API Key | ✅ 已实现 |
| GLM 国内 | Coding Plan 窗口 | `coding_plan.rs` → `open.bigmodel.cn` quota | ✅ 已实现 |
| GLM 国际 | Coding Plan 窗口 | `coding_plan.rs` → `api.z.ai` quota | ✅ 已实现 |
| MiniMax 国内 | Token Plan | `coding_plan.rs` → `api.minimaxi.com` | ✅ 已实现 |
| MiniMax 国际 | Token Plan | `coding_plan.rs` → `api.minimax.io` | ✅ 已实现 |
| SiliconFlow 国内 | 账户余额 | `balance.rs` → `api.siliconflow.cn/v1/user/info` 取 `data.totalBalance`（CNY） | ✅ 已实现 |
| SiliconFlow 国际 | 账户余额 | `balance.rs` → `api.siliconflow.com/v1/user/info` 取 `data.totalBalance`（USD，与国内 Key 不通用） | ✅ 已实现 |
| StepFun | 账户余额 | `balance.rs` → `api.stepfun.com/v1/accounts` 取 `balance`（CNY） | ✅ 已实现 |
| OpenRouter | Credits | `balance.rs` → `openrouter.ai/api/v1/credits` 取 `total_credits - total_usage`（USD，Decimal 减法） | ✅ 已实现 |
| Novita | 账户余额 | `balance.rs` → `api.novita.ai/v3/user/balance` 取 `availableBalance`（单位 0.0001 USD，Decimal 除以 10000） | ✅ 已实现 |
| ZenMux | Token Plan | `coding_plan.rs` | 未实现 |
| 火山方舟 Coding/Agent Plan | Token Plan | `coding_plan.rs` | 不做（需要 AK/SK 两段凭据，当前 Source 编辑器只有一把 API Key） |

已实现的 5 个纯余额平台（硅基流动国内/国际、StepFun、OpenRouter、Novita）共用 `providers/balance.rs` 的 Source adapter，各自独立 Source id（`siliconflow-balance-api` 等）；添加平台后只挂 `balance` capability，不复制 React 页面。

首批要先做完、并出现在「添加平台」里的：DeepSeek、Kimi、GLM、MiniMax。其余按阶段 3 陆续加入同一注册表。

### 检测本机登录（不是网页登录）

| 平台 | 查询内容 | 说明 |
| --- | --- | --- |
| GPT / Codex | 5 小时/7 天窗口、计划、接口返回的 Credits | 默认检测本机 `~/.codex`，不必开网页。可再登录额外 ChatGPT 账号（独立 Codex 目录，不覆盖 CLI） |
| Claude Code | 会话/周窗口 | 本机 Claude 登录与 `/usage` |
| Gemini CLI | 订阅窗口 | 有本机凭据时再开放 |

### 需要网页登录补字段

| 平台 | 额外 Source | 说明 |
| --- | --- | --- |
| DeepSeek | 网页用量与缓存 | API Key 只覆盖余额；Token/缓存/消费走网页会话 |
| GLM | 个人账户余额 | 官方 Coding Plan 不含按量账户余额；隔离登录窗捕获控制台会话 |
| MiMo | 网页会话余额 | 无官方余额接口；隔离登录窗读取含 httpOnly 的 Cookie |

DeepSeek 被用户添加后，应同时出现「官方余额（API Key）」和「网页用量（登录）」两张 Source 卡片。用户可以只配其中一张。Kimi 添加后同时出现「Coding Plan」和「个人余额」两张 API Key 卡片（两套官方 Key，互不替代）。GLM 国内添加后同时出现「Coding Plan」和「网页个人余额」。

## 4. 平台中心交互

骨架仍是双 Tab + 按能力渲染，不按平台复制整页。

### 平台目录

- 只列出用户已添加的平台。
- 空目录显示「添加要监控的平台」。
- 提供「添加平台」：打开注册表多选；已添加的项标记为已接入，不可重复添加。
- 目录项仍显示聚合状态：正常、部分可用、需配置、异常。

### 额度与用量

- 只展示该平台已有快照。
- 已添加但未配凭据：空态，引导到 `接入与来源`。
- 刷新只打已配置 Source。
- 单 Source 失败不影响其他 Source。

### 接入与来源

- 展示该平台注册表声明的全部 Source 卡片，含待配置。
- 默认动作是「编辑来源」填 API Key 并验证保存。
- `web_session` 显示网页登录；本机 Codex 默认「检测并刷新」；额外 ChatGPT 账号才拉起官方登录。
- 清除凭据、移除平台都要二次确认。清除本机 Codex 会退出 CLI 登录；清除额外账号不影响 `~/.codex`。

## 5. GPT 刷新失败怎么理解

GPT 被添加后默认走本机 Codex OAuth，直接检查 `~/.codex` 登录，不是必须先网页登录。若要同时监控第二个 ChatGPT 账号，再添加额外账号：用官方 `codex login` 写入独立目录，额度按 Source 分开展示。

刷新：`codex app-server` → 不行再回退 `https://chatgpt.com/backend-api/wham/usage`。

若只看到 chatgpt.com 连不上，说明本机 app-server 也没读到额度，并且 WHAM 请求没发出去（网络/代理/TLS）。错误文案会同时带上 app-server 失败原因。这不是要改成网页抓 ChatGPT。

Windows 上必须启动 `codex.cmd`，不能误跑 PATH 里那个无扩展名的 Unix 脚本。`接入与来源` 提供「重新登录 Codex CLI」和「清除本机登录」。

## 6. 明确不做

- 任意粘贴第三方中转 Base URL，或添加注册表以外的平台。
- 把本应用做成 cc-switch 式 Provider 切换器、代理或 MCP 管理。
- 用网页登录去抓已经有官方接口的余额/Token Plan。
- 启动时默认创建全部平台账户。
- 用设计稿数字填额度。
- 登录窗读取其他浏览器或其他应用的缓存。
