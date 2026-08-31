# DeepSeek 用量展示重设计 V1

本目录归档 2026-09-01 的 DeepSeek 悬浮页与平台中心「额度与用量」重设计。当前只完成设计与实施说明，尚未修改 React、Rust、SQLite 或 ViewModel。

## 资产

| 文件 | 用途 |
| --- | --- |
| `01-hoverbar-light-concept.png` | DeepSeek 悬浮页浅色方案 |
| `02-platform-center-light-concept.png` | DeepSeek 平台中心浅色方案 |
| `03-metric-icons-target-final.png` | Flash / Pro / 靶心缓存图标最终方向 |
| `references/01-current-hoverbar.png` | 改造前悬浮页 |
| `references/02-current-platform-center.png` | 改造前平台中心 |
| `references/03-inspiration-deepseek-monitor.png` | 用户提供的信息层级参考，只借鉴思路 |
| `references/04-icon-feedback-crop.png` | 首轮图标反馈区域 |

设计图数字只用于说明布局，生产实现必须读取真实脱敏 ViewModel，禁止硬编码示例值。

## 设计结论

### 悬浮页

顺序固定为：平台头 → 资金概览 → V4 Flash / V4 Pro 模型用量 → 靶心缓存效率 → 新鲜度。余额是主值，今日/本月消费是次级值；不展示累计消费、请求明细或趋势图。

### 平台中心

移除每个字段一张同权重大卡片的平铺方式，改为：

1. 账户与来源状态条。
2. 资金概览：余额、今日、本月、累计消费。
3. 模型用量：V4 Flash、V4 Pro 两条横向模型行。
4. 调用与缓存效率：缓存命中率、请求数、输入/输出 Token。
5. 近 7 日消费趋势：只使用真实 `usage_trend` 人民币消费序列。

不实现参考图中的模型单价和每日缓存柱状图，因为现有 ViewModel 没有对应真实字段。

## 图标

- Flash：蓝青晶体闪电。
- Pro：紫色六边神经网络核心。
- 缓存命中率：蓝青同心靶心数据环；按用户反馈不使用分层数据块。

PNG 只作为视觉基准。实现时使用小型 React SVG/CSS 图标组件，不把整张图作为雪碧图加载。

## Capability 映射

| Capability | 平台中心 | 悬浮页 |
| --- | --- | --- |
| `balance` | 资金主值 | 资金主值 |
| `today_spend` / `month_spend` | 资金次级值 | 资金次级值 |
| `total_spend` | 存在才展示 | 不展示 |
| `model_usage_v4_flash` / `model_usage_v4_pro` | 模型行 | 模型行 |
| `request_count` / `prompt_tokens` / `response_tokens` | 紧凑统计 | 不展示 |
| `cache_hit_tokens` / `cache_miss_tokens` | 缓存统计 | 不在前端重新汇总 |
| `cache_hit_rate` | 靶心效率卡 | 靶心效率行 |
| `usage_trend` | 近 7 日消费趋势 | 不展示 |

## 数据与状态边界

- `fresh` 展示真实值；`stale` 保留最后成功值并显示低饱和蓝灰缓存提示。
- `missing` 显示「未获取/暂不可用」，空轨道，不补零。
- 一个 Source 失败不能清空其他 Source 的成功值。
- 金额继续使用后端 Decimal 文本，前端不做浮点汇总或余额差分。
- 不从模型 Token 推导价格，不生成趋势或缓存日序列。

## 响应式

- 平台中心沿用 `useContainerWidth`：wide 双栏，medium 自适应，compact 单栏。
- 悬浮页覆盖顶部/底部约 420px 与左右约 300px；只压缩间距和图标，不隐藏余额、两个模型值或缓存命中率。
- 四边必须满足 `scrollWidth == clientWidth`。

## 实现入口

- `apps/desktop/src/features/platform-center/UsageView.tsx`
- `apps/desktop/src/features/platform-center/CapabilityCard.tsx`
- `apps/desktop/src/features/platform-center/UsageTrend.tsx`
- `apps/desktop/src/features/hoverbar/HoverbarPlatformCard.tsx`
- `apps/desktop/src/features/hoverbar/HoverbarPreview.tsx`
- `apps/desktop/src/styles/global.css`
- `apps/desktop/src/styles/tokens.css`

在统一 `UsageView` 内增加 capability composition renderer，不复制整张 DeepSeek 页面，保持 D006「前端按能力渲染」。

完整实施要求见 [`IMPLEMENTATION_PROMPT.md`](./IMPLEMENTATION_PROMPT.md)。

## 验收与回滚

- 轻量验证：typecheck、build、420/300 四边预览、约 1600×1024 Tauri 主窗口人工检查。
- 本轮预计 Rust 零改动；若修改 ViewModel 再执行 cargo check。
- 回滚只恢复 React/CSS 旧渲染，不删除快照、账号、Source 或凭据。
