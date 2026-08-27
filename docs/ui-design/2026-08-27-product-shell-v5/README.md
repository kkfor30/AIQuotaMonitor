# 产品外壳 V5：总览、GPT 雷达、设置与悬浮球

## 设计稿

| 文件 | 页面与验证目标 |
| --- | --- |
| `04-overview.png` | 总览；跨平台状态、需要关注、窗口压力、DeepSeek 消费和最近刷新 |
| `05-gpt-reset-radar-summary.png` | GPT 雷达 / 信号摘要；综合判断、额度上下文、独立证据链和检查历史 |
| `06-gpt-reset-radar-tibo-detail.png` | GPT 雷达 / Tibo 动态；列表筛选、原文、翻译、规则命中和证据操作 |
| `07-gpt-reset-radar-rules-ai.png` | GPT 雷达 / 规则与 AI；可视化条件、权重、回测、AI 影响上限和审计日志 |
| `08-settings-hoverbar.png` | 设置 / 悬浮球；二级设置导航、停靠、触发、排序和实时预览 |
| `09-hoverbar-state-matrix.png` | 悬浮球组件状态；明暗主题、渐进展开、部分失败、雷达提醒和四边适配 |

## 页面职责

### 总览

- 只回答“当前整体是否健康、哪里需要处理、哪个窗口值得关注”。
- 不承担平台凭据配置，不复制平台中心的完整详情。
- 不把百分比、Token 和金额混到同一张趋势图。
- 点击 DeepSeek 会话失效进入“平台中心 / 接入与来源”并定位 Source。
- 点击 Kimi 未接入进入 Kimi 的接入页。
- 点击 GPT 信号进入“GPT 重置雷达 / 信号摘要”。

### GPT 重置雷达

内部固定三个 Tab：

1. 信号摘要
2. Tibo 动态
3. 规则与 AI

综合判断必须保留“仅为推测，不代表官方结论”。公开来源、本地规则和可选 AI 独立产出证据，不能由 AI 单独决定结论。

### Tibo 动态

- 列表选择与右侧详情联动，不使用遮挡内容的弹窗。
- 原文、翻译、来源、抓取时间、规则命中和额度关联分开显示。
- “内容相关”不等于“新重置信号”；上一轮内容必须明确标注。
- “加入证据”只改变当前检查的证据集合，不直接修改综合结论。

### 规则与 AI

- 使用可视化条件构建器，不在 MVP 暴露任意脚本编辑器。
- 规则拥有优先级、权重、时间窗口、冷却期、启用状态和版本历史。
- 保存规则前展示最近 30 天回测摘要。
- AI 默认关闭，影响最终结论的权重有上限。
- 发给 AI 的数据仅允许公开文本与脱敏摘要，禁止发送凭据和 Cookie。

### 设置

二级分区固定为：

- 常规
- 外观
- 悬浮球
- 刷新与通知
- 数据与隐私
- 关于

平台登录和 API Key 不进入设置，统一留在“平台中心 / 接入与来源”。危险操作只出现在“数据与隐私”，不能常驻所有设置页。

### 悬浮球

渐进层级：

```text
小球 40×40
→ 轻展开：平台图标和关注数量
→ 平台速览：横向或纵向平台卡片
→ 平台详情 / 雷达提醒
→ 后台查看完整详情
```

悬浮球和主窗口必须消费同一份 ViewModel；悬浮球不独立刷新平台接口。

## 前端组件建议

```text
AppShell
├─ SidebarNavigation
├─ OverviewPage
│  ├─ GlobalStatusSummary
│  ├─ KeyPlatformCards
│  ├─ AttentionList
│  ├─ WindowPressureChart
│  ├─ SpendingTrendChart
│  └─ RefreshHistory
├─ GptResetRadarPage
│  ├─ RadarHeader
│  ├─ RadarTabs
│  ├─ SignalSummaryView
│  ├─ TiboFeedView
│  └─ RulesAiView
├─ SettingsPage
│  ├─ SettingsSectionRail
│  └─ HoverbarSettingsView
└─ HoverbarApp
   ├─ HoverbarAnchor
   ├─ HoverbarPeek
   ├─ HoverbarPlatformCarousel
   └─ HoverbarDetail
```

## 关键交互

1. 全部刷新只触发已配置 Source；待配置平台不发网络请求。
2. 总览关注项点击后携带 `platformId/sourceId` 定位目标，不让用户再次寻找错误位置。
3. GPT “立即检查”展示抓取、规则计算和可选 AI 三个阶段的独立进度。
4. Tibo Feed 切换选中项只更新右侧详情，保留筛选和滚动位置。
5. 规则保存创建新版本；回测不修改正式规则。
6. 设置项默认自动保存；网络或磁盘失败时保留未提交值并提供重试。
7. 悬浮球吸附方向决定展开方向，内容顺序不因方向改变。
8. Source 失败时成功数据继续显示；缓存数据必须带 `stale` 和最后成功时间。

## 状态与颜色

- 蓝色：选中、主操作、普通数据。
- 绿色：数据新鲜或检查成功。
- 橙色：观察中、部分可用、缓存可能过期。
- 红色：明确失败或危险操作。
- 状态必须同时包含文字或图标，不能只依赖颜色。

## 示例数据说明

设计稿中的 `42%`、`Credits $12.34`、`可重置次数 2`、Tibo 文本和时间均为交互验证数据，不代表已经存在对应后端接口。前端只能展示 ViewModel 实际返回的字段；缺失时显示未提供或未接入，不能用设计稿数字补位。

## 生成方式

本组设计使用内置 imagegen 串行生成。每一张都以前一张已确认外壳为主参考，并使用旧稿作为功能参考；旧版七项一级导航、页面职责和大段样式不应被复刻。
