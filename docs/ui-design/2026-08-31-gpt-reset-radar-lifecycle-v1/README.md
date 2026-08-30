# GPT 重置雷达生命周期 V1：实现计划与悬浮详情设计

更新时间：2026-08-31

## 1. 最终产品方向

GPT 重置雷达不再用一条“最新成功 AI 结论”覆盖所有内容，而是并行展示三路证据，并由一轮重置事件串联：

```text
CodexRadar 来源判断（始终可用，独立 freshness）
+ 用户配置 AI 对 Tibo 原帖的判断（开关只控制这一层）
+ 本机 Codex 额度观察（可因网络不可达而缺失）
→ 重置事件生命周期
```

三路证据互不覆盖、互不伪装：

- CodexRadar 顶部公告始终保留并标明来源；AI 关闭不能把它替换成历史 AI 结论。
- AI 只接收 Tibo 英文原文、发布时间、原帖链接和当前事件已有原文上下文，不接收 CodexRadar 公告判断、中文翻译、额度、凭据或 Cookie。
- 本机额度观察使用确定性 Rust 逻辑，不进入 AI 输入；网络不可达表示 `unavailable`，不能解释成“没有重置”。
- “来源称已落地”和“本机观察到额度刷新”必须分开，后者也只能代表本机账号。

## 2. 事件与状态模型

### 2.1 事件阶段

```text
watching          观察中
upcoming          出现较强的即将重置信号
landed_claimed    来源声称已按下按钮
landed_observed   本机账号观察到额度刷新
closed            事件完成、取消或失效
```

事件阶段不是简单的高/中/低把握度。AI 输出应拆为：

- `eventRelation`: `new_event | same_event | none`
- `eventPhase`: 上述事件阶段
- `deltaEffect`: `reinforce | no_change | weaken | advance_phase | cancel | new_event`
- `signalLevel`: `none | weak | strong`
- `contextStatus`: `complete | context_missing | conflicting`
- `conclusion` / `analysisBasis` / `citations` / `support` / `against` / `uncertainty`

历史 `confidence` 字段可保留兼容，但新 UI 不再展示“高把握/低把握”。

### 2.2 本机额度验证状态

```text
unavailable        当前网络无法获取 Codex 额度
insufficient_data  缺少事件前后成功快照
pending            等待事件后的下一次成功额度刷新
scheduled          到达原定时间后的正常周期刷新
possible_reset     比例恢复但结构化证据不足
unscheduled_reset  未到原定时间，窗口恢复且 reset_at 明显后移
no_change          已成功刷新，但本次未观察到窗口变化
```

归因单独保存：

```text
unknown | scheduled | user_confirmed | radar_correlated
```

`unscheduled_reset` 只能显示“本机观察到非计划额度刷新”。只有事件时间落在前后快照区间内时才可标 `radar_correlated`，仍不能写成官方全局结论。

## 3. 数据与后端实施计划

### 3.1 SQLite v6

1. `capability_snapshots` 增加结构化额度窗口字段：
   - `window_seconds INTEGER`
   - `reset_at INTEGER`
   - 继续使用现有 `progress` 保存剩余比例；禁止解析 `secondary_value` 中文文本做业务判断。
2. `radar_analyses` 增加：
   - `event_id`
   - `analysis_mode`（`delta | rebuild`）
   - `context_hash`
   - `prompt_hash`
   - `event_relation`
   - `event_phase`
   - `delta_effect`
   - `signal_level`
   - `context_status`
3. 新增 `radar_events`：事件阶段、首次信号、最新证据、来源声称落地时间、本机观察时间、关闭时间、原因摘要。
4. 新增 `radar_event_evidence`：`event_id + post_id + relation + analysis_id + added_at`，同一事件关联跨日原帖。
5. 老分析记录保留为历史分析，事件字段为空；不反推、不伪造生命周期。

### 3.2 Codex 额度快照

- `providers/codex.rs` 在解析窗口时同时保留 `duration_seconds` 和结构化 `reset_at`。
- 扩展 `CapabilityData` / `SnapshotRecord` 的可选窗口字段；非窗口平台写 `None`，不改变现有金额与 Token 能力。
- 额度检测器直接读取原始 `capability_snapshots`，不使用每日趋势聚合。
- 比较同账号、同 Source、同窗口的相邻成功快照；按计划内、非计划、证据不足分类。
- 多账号分别输出观察状态，不用一个账号的变化代表全部账号。

### 3.3 AI 增量分析

- 用户选择“当天/3 天/7 天”决定本次新增动态范围。
- 当前活动事件已关联的关键原帖作为“事件上下文”单独带入，即使位于所选范围之前；输入预览必须分组显示“本次新增”和“事件上下文”。
- 模型判断新帖对当前事件的影响，普通回帖输出 `no_change`，不能重新覆盖此前阶段。
- 分析复用键至少包含：新增原文 hash、上下文 hash、模型、`prompt_version`、用户语义提示 hash。
- `latest_radar_analysis` 不再作为全局当前结论；ViewModel 返回当前事件匹配分析、最近历史成功分析和最新失败状态。

### 3.4 ViewModel

`RadarSnapshot` 增加：

- `sourceAssessment`：CodexRadar 公告、来源时间、freshness。
- `event`：当前事件、阶段、时间线节点。
- `aiAssessment`：开关、当前/历史/失败/待分析状态、结构化输出。
- `quotaVerifications[]`：按 GPT 账号返回验证状态、前后值、重置时间、观察区间和归因。

保留现有 `posts`、`checks`、`analysisPrefs`，避免一次性重写页面数据流。

## 4. 前端信息架构

### 4.1 GPT 卡底摘要条

摘要条只给快速状态，不展开分析依据：

```text
重置雷达  [来源称已落地]  查看详情
CodexRadar  按钮今日已按下，庆祝延至明日
AI 辅助     已关联昨日预告 / 已关闭 / 有新动态待分析
本机额度    暂无法验证 / 待验证 / 观察到非计划刷新
更新时间
```

- AI 关闭：AI 行显示“已关闭”，可附最近历史分析时间，但历史正文不能替换 CodexRadar 行。
- AI 失败：显示“本次分析失败 · 展示历史分析时间”，来源行不受影响。
- 本机额度网络失败：中性灰或弱琥珀，不把雷达整体标红。
- 删除用户可见的“结论可能过期”和高/低把握徽章。

### 4.2 悬浮雷达详情

固定顺序：

1. 当前重置事件
2. 两路判断（CodexRadar / AI 辅助）
3. 本机额度验证
4. 事件时间线
5. 当前事件相关动态

设计稿：

- `01-top-docked-radar-lifecycle.png`：顶部/底部宽停靠完整页。
- `02-side-docked-radar-lifecycle.png`：左右侧边窄停靠兼容页。

图片是信息架构和视觉层级参考；实现必须使用真实 React/CSS、现有图标和 ViewModel，不把设计稿作为整页位图。

## 5. 顶部/底部与左右侧兼容规则

### 顶部/底部宽停靠

- 保留“返回额度 + GPT 重置雷达 + 刷新 + 仅为推测”。
- 所有卡片单列，不做双栏；两路判断在同一卡内分两行。
- 完整显示本机额度上次成功值、原定重置时间和重试按钮。
- 时间线使用横向三节点或紧凑竖向列表，内容区内部滚动。

### 左右侧边窄停靠

- 隐藏“GPT 重置雷达”标题，只保留“返回额度 / 刷新 / 仅为推测”。
- 顶部操作组必须单行完整，不使用省略号、不裁切按钮。
- 当前事件卡保留阶段、短结论和时间范围；长原因进入后续卡。
- 两路判断改成标签 + 可换行正文的两条纵向行。
- 本机额度验证只展示状态、短原因、上次成功和紧凑“重试”；详细前后对比进入内部滚动或主窗口。
- 时间线放在内部滚动下方，首屏只需露出标题作为继续滚动提示。
- 正文不小于现有字号，不以缩小字体解决宽度问题；可点击文案保持单行。
- `left/right/top/bottom` 四形态均必须满足 `scrollWidth == clientWidth`，只允许内容区纵向滚动。

## 6. 状态验收矩阵

至少覆盖：

1. AI 关闭、无历史分析。
2. AI 关闭、有历史分析、有新帖子。
3. AI 开启、当前分析匹配。
4. AI 开启、新动态待分析。
5. 最新分析失败、旧成功分析保留。
6. CodexRadar 同步失败、来源缓存 stale。
7. GPT 额度网络不可达，雷达与 AI 正常。
8. 缺少额度前置快照。
9. 正常计划内 7 天窗口刷新。
10. 非计划窗口刷新，与事件时间相关。
11. 多账号一部分观察到刷新、一部分未观察到。
12. 四种停靠方向无横向溢出。

## 7. 验证与边界

- 轻量验证：`cargo check`、桌面端 `typecheck`、`build`。
- 使用 `hoverbar-preview.html` 构造上述关键状态，Playwright 检查四边形态 `scrollWidth == clientWidth`。
- 手工检查顶部、底部、左侧、右侧的返回/刷新/查看详情/内部滚动。
- 不进行深度测试，不访问或修改用户凭据，不把额度数据发送给 AI。

