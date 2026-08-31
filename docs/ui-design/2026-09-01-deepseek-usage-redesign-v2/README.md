# DeepSeek 模型与缓存展示重设计 V2

本目录基于新增的 V4 Flash Vision 和分模型缓存能力，修正 V1 的模型语义、图标规则与悬浮页数据边界。V2 取代 V1，当前仍是设计与实施说明，未修改代码。

## 资产

| 文件 | 用途 |
| --- | --- |
| `01-platform-center-model-cache-light.png` | 平台中心最终稿：标题仪表 + 总/分模型靶心 |
| `02-hoverbar-total-cache-light.png` | 顶部/底部宽停靠最终稿（约 420px） |
| `03-hoverbar-side-docked-light.png` | 左/右侧边停靠最终稿（约 300px） |
| `references/01-current-platform-center.png` | 新功能落地后的当前看板 |
| `references/02-current-hoverbar.png` | 新功能落地后的当前悬浮页 |

## 代码核对结论

- 悬浮页按 `余额 → 今日/本月消费 → 三个模型 Token → 总缓存命中率` 渲染；只消费总 `cache_hit_rate`，不消费三个分模型命中率。
- 平台中心同时展示三个模型 Token、总 `cache_hit_rate` 和三个 `model_usage_*_cache_hit_rate`。
- 当前 `V4 Flash Vision` 复用了 Flash 图标。
- 当前平台中心分模型缓存行复用了模型图标；应统一使用缓存靶心图标。

对应入口：

- `apps/desktop/src/features/hoverbar/HoverbarPlatformCard.tsx`
- `apps/desktop/src/features/platform-center/DeepSeekUsageSummary.tsx`
- `apps/desktop/src/components/ui/MetricIcons.tsx`
- `apps/desktop/src-tauri/src/providers/deepseek/web_usage.rs`

## 模型语义与图标

| 模型 | 产品语义 | 图标方向 |
| --- | --- | --- |
| V4 Flash | 旗舰版轻量模型 | 蓝青晶体翼/速度火花；不能只表现成普通闪电 |
| V4 Flash Vision | 视觉模型 | 青色相机光圈/视觉棱镜；禁止复用 Flash 图标 |
| V4 Pro | 深度思考模型 | 紫色分层神经旋涡/推理核心；强调深度思考 |
| 缓存命中率 | 总体或分模型命中效率 | 蓝青靶心数据环；模型名靠文字区分 |

模型身份图标只用于「模型用量」。平台中心右侧的图标分三层：

- 面板标题「调用与缓存效率」：独立的深蓝效率仪表图标，不能使用靶心。
- 「总缓存命中率（全部模型）」：较大的 `TargetRingIcon`。
- 三个分模型缓存行：完全相同的小 `TargetRingIcon`，模型名靠文字区分。

## 悬浮页

悬浮页真实数据边界：

1. 个人余额。
2. 今日消费、本月消费。
3. V4 Flash、V4 Flash Vision、V4 Pro 本月 Token。
4. **总缓存命中率（全部模型）**、总命中/输入说明。

不展示分模型请求/输入/输出或分模型命中率。总缓存必须是模型列表后的独立全宽块，不能并入 V4 Flash 行。

模型副标题固定为：

- V4 Flash：`旗舰轻量模型 · 本月 Token`
- V4 Flash Vision：`视觉模型 · 本月 Token`
- V4 Pro：`深度思考模型 · 本月 Token`

### 停靠适配

- 顶部/底部：约 420px，余额与消费可横向组合，模型行保留完整副标题。
- 左侧/右侧：约 300px，余额独占一行，今日/本月双列，模型图标缩至 26–30px，副标题只保留模型语义；总缓存仍为独立全宽块。
- 侧边内容区内部纵向滚动，页脚固定；小球固定在物理屏幕边缘，详情向内展开。
- 四边均不得隐藏三模型或总缓存，必须 `scrollWidth == clientWidth`。

## 平台中心

保留模型/缓存双面板，因为数据职责不同：

- 左侧「模型用量」：三个模型使用各自身份图标、语义副标题和 Token。
- 右侧「调用与缓存效率」：标题使用效率仪表；顶部总命中率和下方三个分模型缓存行使用靶心图标。
- 分模型没有调用时显示 `未调用`，不显示空进度条，不突出 `0%`。
- 两个面板按内容高度收口，三行对齐，避免左侧大面积空白。
- 近 7 日消费趋势继续独立保留。

## 状态边界

- 真实 0 Token 可以显示 `0`；分模型缓存 `primary/secondary` 都为空时显示 `未调用`。
- `missing` 显示 `未获取/暂不可用`，不得当作 0。
- `stale` 保留最后成功值和缓存时间。
- 金额与 Token 使用后端格式化文本；前端不汇总、不推导价格。

## 修改计划

1. 图标层：补 `VisionApertureIcon`、重绘 Flash/Pro，新增 `EfficiencyGaugeIcon`，保留 `TargetRingIcon`。
2. 平台中心：三模型映射独立身份图标；缓存面板标题改仪表，所有缓存值统一靶心；收紧双面板高度。
3. 悬浮页：维持总缓存数据边界；分别实现 420px 宽停靠与 300px 侧边停靠密度。
4. 验收：更新 HoverbarPreview，检查浅/深、四边、fresh/stale/missing/未调用，执行 typecheck/build 和零横向溢出检查。

完整实施要求见 [`IMPLEMENTATION_PROMPT.md`](./IMPLEMENTATION_PROMPT.md)。
