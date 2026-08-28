# 平台中心 V4：设计验证与前端交接

## 本轮结论

左侧一级导航固定为四项：

1. 总览
2. 平台中心
3. GPT 重置雷达
4. 设置

旧版“模型额度”和“账户与套餐”合并为“平台中心”。平台中心内部再按用户任务拆成两个 Tab：

- 额度与用量：查看已接入平台的业务数据、刷新结果和异常状态。
- 接入与来源：配置某个平台的数据来源、凭据、验证状态和能力覆盖。

这样不会把“查看数据”和“配置接入”混成两个一级页面，也能保持平台模板可扩展。

## 设计稿

| 文件 | 验证目标 |
| --- | --- |
| `01-platform-center-deepseek-usage.png` | DeepSeek 正常态；额度、余额、消费、Token 与缓存统计如何组织 |
| `02-platform-center-deepseek-sources.png` | 数据来源拆分；编辑来源时使用覆盖式右侧抽屉 |
| `03-platform-center-deepseek-partial-stale.png` | 单 Source 失败；成功数据保留，失败数据使用上次成功快照并明确标记 |
| `10-platform-center-empty.png` | 尚未添加任何平台；空目录与「添加平台」引导 |
| `11-platform-center-add-catalog.png` | 从产品注册表勾选要监控的平台，不是自定义中转 URL |
| `12-platform-center-setup-apikey.png` | 添加平台后立刻展示接入表单。设计稿只画了 API Key；实现须同时保留 cc-switch 同款字段：供应商名称、备注、官网链接、获取 API Key、官方 API 请求地址（完整 URL） |

## 页面职责

### 总览

- 汇总当前需要关注的平台，不承担凭据配置。
- 显示可用平台数、异常平台数、即将重置窗口、余额预警和最近刷新。
- 点击平台摘要进入“平台中心 / 额度与用量”。

### 平台中心 / 额度与用量

- 左侧平台目录只展示用户已添加的平台；通过「添加平台」从产品注册表选择，不支持任意中转 URL。
- 目录项显示平台级聚合状态：正常、部分可用、需配置、异常。空目录引导用户添加平台。
- 顶部显示当前平台、接入方式摘要、刷新当前平台和打开官方页面。
- 主体由平台能力决定，公共框架不强制所有平台展示相同卡片。
- DeepSeek 专属模块：余额、今日消费、本月消费、模型 Token、缓存命中、趋势和刷新记录。
- GPT/Codex 后续使用相同外壳，但主体替换为窗口额度、Credits、可重置次数和重置信号摘要。

### 平台中心 / 接入与来源

- 一个“来源”对应一个独立获取能力，不等同于整个平台。
- DeepSeek 至少拆成“API 余额”和“模型用量与缓存”两个 Source。
- 每个 Source 独立保存：来源类型、凭据状态、最后验证、最后成功、当前错误、能力覆盖。
- 编辑使用右侧覆盖抽屉；打开抽屉时主内容尺寸不变化。
- 保存前先验证；验证成功后保存配置并触发该 Source 刷新。
- 接入表单保留官网链接和官方 API 请求地址（预填完整 URL）；「管理与测速」打开官网，不做代理测速。
- 清除凭据是高风险操作，需要二次确认，并说明会失去哪些能力。

### GPT 重置雷达

- 一级页面只服务 GPT 重置判断，不承载通用额度配置。
- 内部固定为：信号摘要、Tibo 动态、AI 辅助分析。
- V1 从 Codex Radar 公开页面同步其转载的 Tibo 英文原文；直接访问 X 作为后续可替换 Source。
- 摘要进入悬浮球；来源状态、英文原文、原帖链接、用户配置 AI 引用和推导详情留在后台。

### 设置

- 外观主题、语言、启动项、通知、刷新频率和缓存策略。
- 悬浮球/灵动岛的显示器、停靠边缘、默认形态、展开方式和平台排序也放在此处。

## 前端组件建议

```text
PlatformCenterPage
├─ ProviderRail
│  └─ ProviderRailItem
├─ ProviderHeader
├─ PlatformTabs
├─ UsageView
│  ├─ SourceHealthSummary
│  ├─ CapabilityCard[]
│  ├─ UsageTrend
│  └─ RefreshHistory
├─ SourcesView
│  ├─ SourceCard[]
│  └─ CapabilityCoverage
└─ SourceEditorDrawer
```

不要按 GPT、DeepSeek、Claude 分别复制整页。页面骨架保持一致，平台通过模板声明它支持的能力与对应渲染器。

## 最小数据模型

```ts
type DataFreshness = 'fresh' | 'stale' | 'missing';
type SourceState = 'ready' | 'refreshing' | 'auth_required' | 'error';

interface PlatformViewModel {
  providerId: string;
  displayName: string;
  aggregateStatus: 'healthy' | 'partial' | 'setup_required' | 'error';
  capabilities: CapabilitySnapshot[];
  sources: DataSourceViewModel[];
}

interface CapabilitySnapshot {
  capabilityId: string;
  sourceId: string;
  freshness: DataFreshness;
  capturedAt?: string;
  value?: unknown;
  lastGoodValue?: unknown;
}

interface DataSourceViewModel {
  sourceId: string;
  sourceType: 'api_key' | 'web_session' | 'local_cli' | 'oauth';
  state: SourceState;
  lastValidatedAt?: string;
  lastSuccessAt?: string;
  errorCode?: string;
  errorMessage?: string;
}
```

## 状态与交互规则

1. 切换平台时保留当前内部 Tab；若目标平台不支持该 Tab，再回退到“额度与用量”。
2. “刷新当前平台”并行刷新该平台所有已配置 Source，单个 Source 失败不取消其他结果。
3. 某 Source 刷新失败时：
   - 该 Source 状态变为错误或需登录；
   - 有上次成功值则继续展示，并标记“缓存数据”“可能过期”和最后成功时间；
   - 没有上次成功值才显示空状态，禁止补零或生成虚假数据；
   - 平台聚合状态变为“部分可用”，而不是整页失败。
4. 未配置平台显示“需配置”和接入引导，不显示示例余额、额度或消费数据。
5. 点击错误摘要跳转到“接入与来源”并定位对应 Source。
6. 刷新按钮需要有进行中、成功短反馈和失败结果；重复点击应合并或阻止并发请求。
7. 所有金额使用后端返回的币种与精度，不能用浮点运算拼接消费汇总。

## 视觉实现约束

- 主内容以普通布局和卡片组件实现，设计图不能作为页面背景。
- 玻璃效果只用于容器层次：低透明白底、细描边、轻阴影；正文对比度优先。
- 蓝色表示主操作与选中态，绿色表示成功，橙色表示部分可用/缓存，红色只表示明确失败。
- 状态不能只依赖颜色，必须同时有图标或文字。
- 平台目录、页面头部和内部 Tab 在三种状态下尺寸不跳动。

## 本轮未覆盖

- GPT/Codex 平台专属详情稿。
- GPT 重置雷达与 Tibo 详情的重构稿。
- 悬浮球与后台之间的联动状态稿。
- 实际前端代码；当前工作区只有设计资产，没有文档中曾提及的生产项目目录。

## 生成提示词摘要

- 主稿：基于旧额度页、账户页和总览页，重构为四项一级导航、平台目录、双内部 Tab，并补齐 DeepSeek 专属数据。
- 接入稿：保持主稿外壳不变，切换到“接入与来源”，拆分 API 余额与网页登录会话，并打开覆盖式编辑抽屉。
- 异常稿：保持额度页不变，仅让网页登录 Source 失效，保留上次成功 Token/缓存快照，余额来源继续正常。
