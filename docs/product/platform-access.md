# 平台接入需求

本文说明平台中心如何接入数据来源。它不改变产品定位：本应用是额度监控中心，不是 Provider 路由、代理或「添加任意模型服务商」工具。

## 1. 平台中心现在的问题

当前左侧平台目录是产品模板，这是对的。问题在接入交互：

1. GLM、Kimi、MiMo、MiniMax、Claude Code 仍是空占位：`接入与来源` 没有 Source 卡片，文案是「该平台暂未提供可配置来源」。用户无法开始接入。
2. 未配置平台缺少接入引导：没有说明要填 API Key、检测本地 CLI，还是打开网页登录。
3. GPT 刷新失败会把整页打成「异常」。本次本机错误是 `chatgpt.com/backend-api/wham/usage` 发不出请求；这是 Codex 本地协议失败后的 WHAM 回退网络失败，不是「平台中心不会加平台」。
4. DeepSeek 网页用量属于「无官方接口、靠登录抓字段」。不能把这种抓取扩成任意平台的通用脚本。

平台目录不提供「添加平台」。接入发生在某个已有平台的 `接入与来源` 里，按 Source 配置。

## 2. 两条接入路径

每个 Source 只走其中一条。前端只渲染脱敏 ViewModel，不解析平台原始响应。

### A. 官方查询（默认）

有官方或准官方额度/余额接口，或本机 CLI 已登录可读额度时，用这条路径。

实现参考只读仓库 `D:\AIproject\cc-switch`：

| 能力 | cc-switch 文件 |
| --- | --- |
| DeepSeek 等官方余额 | `src-tauri/src/services/balance.rs` |
| Kimi / GLM Coding Plan / MiniMax Token Plan | `src-tauri/src/services/coding_plan.rs` |
| Codex / Claude 等本地订阅额度 | `src-tauri/src/services/subscription.rs` |

改造要求：

- 拆成 `Platform → Account → Source → Capability → Snapshot`，不复制 cc-switch 的 Provider 路由、代理、MCP、Skills、配置接管。
- 金额用 Decimal/文本定点，禁止 `f64` 汇总。
- 凭据进 Windows Credential Manager；本地 CLI/OAuth 只读本机登录态，不把完整 Token 拷进本应用数据库。
- 瞬时失败保留最后成功快照并标 stale；确定性失败展示结构化错误。

凭据形态：`api_key`、`local_cli`、`oauth`。

### B. 会话抓取（仅无官方接口时）

官方没有开放对应额度、余额、用量或缓存字段时，才用隔离登录窗抓会话。

当前已知需要这条路径的能力：

- DeepSeek 模型 Token、请求数、缓存命中/未命中、消费趋势（平台网页 `amount/cost`，不是官方 API Key）。
- GLM 个人账户余额（若官方 Coding Plan 接口不覆盖个人余额）。
- MiMo 网页会话余额（若没有可用官方余额接口）。

规则：

- 登录窗没有主窗口 Tauri IPC。
- 只抓当前平台需要的字段；验证成功后再写入 Credential Manager。
- 登录中途的临时会话若用量全空，不得覆盖上次成功快照。
- 不开放任意 JavaScript 用量脚本，不扫其他应用的浏览器缓存。

凭据形态：`web_session`。

## 3. 首批平台的 Source 模板

平台列表由产品固定，启动时写入模板。未配置时也要在 `接入与来源` 显示这些卡片和接入动作。

| 平台 | Source | 路径 | 能力 | 接入动作 |
| --- | --- | --- | --- | --- |
| DeepSeek | 官方余额 | A，cc-switch/DeepSeek `user/balance` | 账户余额 | 填写并验证 API Key |
| DeepSeek | 网页用量与缓存 | B，网页登录 | Token、请求数、缓存命中、消费、趋势 | 网页登录或粘贴 usage token |
| GPT / Codex | 本地订阅 | A，cc-switch Codex 订阅；app-server 优先，WHAM 回退 | 5 小时/7 天窗口、计划、接口返回的 Credits | 检测本机 Codex OAuth，无需在本应用再登录 ChatGPT |
| Claude Code | 本地 `/usage` | A，cc-switch/Claude 订阅思路 | 会话窗口、周窗口 | 检测本机 Claude Code 登录 |
| GLM | Coding Plan | A，cc-switch `open.bigmodel.cn` / `api.z.ai` quota | Token Plan 窗口 | 填写 API Key（国内/国际按模板区分） |
| GLM | 个人余额 | B，仅当官方接口没有个人余额 | 个人账户余额 | 网页登录 |
| Kimi | 官方余额 | A，cc-switch Moonshot 余额 | 账户余额 | 填写 API Key |
| Kimi | Coding Plan | A，cc-switch `api.kimi.com/coding` | Token Plan 窗口 | 填写 API Key |
| MiniMax | Coding Plan | A，cc-switch 国内 `minimaxi.com` / 国际 `minimax.io` | Token Plan 窗口 | 填写 API Key，按模板区分国内/国际 |
| MiMo | 网页会话或官方余额 | 有官方余额走 A，否则走 B | 余额/额度 | 对应填写 Key 或网页登录 |

一个平台可以同时有 A 和 B。DeepSeek 已是样板：余额走官方 Key，用量走网页登录。

## 4. 平台中心交互

骨架保持现有双 Tab，不按平台复制整页。

### 额度与用量

- 只展示已有快照的能力。
- 未配置：空态 + 引导到 `接入与来源`，不出现「添加平台」。
- 刷新只打已配置 Source。
- 单 Source 失败不影响其他 Source；有上次成功值则 stale，没有则 missing。

### 接入与来源

- 每个产品平台至少一张 Source 卡片，状态可以是待配置。
- 卡片展示来源类型、凭据状态、最后成功、当前错误、覆盖哪些能力。
- 动作按来源类型区分：验证并保存 API Key、网页登录、检测并刷新本地 CLI。
- 编辑仍用右侧抽屉；保存前验证；清除凭据要二次确认。
- 不提供自定义 Base URL 的通用 Provider 表单，不接入 cc-switch 的「添加供应商」。

## 5. GPT 刷新失败怎么理解

GPT Source 是本地 Codex OAuth，不是网页登录 ChatGPT。

刷新顺序：`codex app-server` → 仅当 CLI/协议/网络不可用时回退 `https://chatgpt.com/backend-api/wham/usage`。

当前报错 `error sending request for url (https://chatgpt.com/backend-api/wham/usage)` 表示：

1. 本机 app-server 没有给出可用窗口额度；
2. 回退 WHAM 时请求没有发出去（TLS/代理/DNS/无法访问 chatgpt.com），还没到 HTTP 状态码。

这不是「要像 DeepSeek 那样再做一个 ChatGPT 网页登录」。V1 继续用本机 Codex 登录。若本机访问不了 chatgpt.com，WHAM 回退会失败，需要先让 app-server 可用，或具备访问 ChatGPT 的网络。

## 6. 明确不做

- 用户任意添加平台或粘贴第三方中转 Base URL。
- 把本应用做成 cc-switch 式 Provider 切换器或 API 代理。
- 用网页登录去抓已经有官方接口的余额/Token Plan。
- 用设计稿数字或登录成功假数据填额度。
- 登录窗读取其他浏览器或其他应用的缓存。
