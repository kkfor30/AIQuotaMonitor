# 悬浮详情重构交接

## 设计资产

- `10-hoverbar-detail-top-docked.png`：顶部/底部停靠，一行一个平台；GPT 多账户在同一卡内分组。
- `11-hoverbar-detail-side-docked.png`：左侧/右侧停靠，窄宽度单列卡片。

两张图均为 1600×900 的信息层级设计稿，不是生产窗口的像素尺寸。生产窗口继续遵守现有逻辑尺寸：顶部/底部约 420px 宽，左右侧约 300px 宽；内容超高时列表滚动，不扩大到设计画布尺寸。

## 设计原则

1. 所有停靠方向都一行一个平台，禁止一行并排两个平台。
2. 一个平台注册为一张卡；同平台同时有 Token Plan 和个人余额时仍在同一卡展示。
3. GPT/Codex 多账户不拆成多个平台卡。卡头显示平台名称、账户数量和平台聚合状态，卡内按 Source/账户分组。
4. 每个账户组独立显示账户名、真实 `plan_level`、该次刷新实际返回的窗口、重置时间和账户状态。窗口种类随官方接口变化：有 5 小时就展示 5 小时，只有 7 天就只展示 7 天，有 30 天就展示月窗口；接口没返回的窗口不占行。
5. 默认只允许展示：实际返回的 `quota_window_*`、`balance`、`plan_level`，以及平台/Source 状态。官方不再返回的窗口不写「暂不可用」，整行隐藏。刷新失败时保留最后成功窗口并标 stale。
6. DeepSeek 额外展示 `today_spend`、`month_spend`、`cache_hit_rate`，数字与个人余额同一列对齐。其余仍不展示 Credits、Token 明细、赠送/充值额度、请求数和趋势。
7. 状态必须有文字：正常、部分可用、异常；绿色/橙色/红色只作辅助。
8. 不生成示例值，不把缺失值补成 0、Free 或正常。有最后成功快照时保留值并标记缓存/可能过期；无真实值时显示暂不可用。
9. 顶部/底部共用全宽单列结构，只改变面板入场方向和小球所在边；左右侧共用窄版结构。
10. 不修改悬浮球拖动、吸附、延迟展开/收起、全屏隐藏、主题、平台排序和主窗口共享 ViewModel 的现有语义。

## 多账户结构

```text
GPT / Codex        2 个账户       正常
├─ 本机账户   Plus   正常
│  ├─ 5小时窗口：62%   重置 14:30
│  └─ 7天窗口：81%     重置 09/02 08:00
└─ 工作账号   Free   正常
   └─ 30天窗口：68%    重置 09/28 00:00
```

- 平台状态继续使用 `platform.aggregateStatus`。
- 账户按配置的 Source 分组，能力通过 `sourceId` 归属账户。
- 账户状态：认证失效或刷新失败为异常；有可用套餐或窗口且无过期缓存为正常；缓存过期为部分可用。官方未返回的窗口不是缺失错误，不视为部分可用或异常。
- 套餐只读取同一 Source 的 `plan_level`，不得根据账号位置或名称推断 Plus/Pro/Free。Plus/Pro/Free 使用同一主色胶囊。
- 重置时间从窗口能力 `value.secondary` 中的真实“重置…”片段展示；没有真实重置时间就不显示。

## 实现位置

- `apps/desktop/src/features/hoverbar/HoverbarPlatformCard.tsx`
  - 重写卡片数据筛选、平台/账户分组和允许字段格式化。
  - 删除 `credits`、今日/月度消费、赠送/充值余额等悬浮详情格式化分支。
- `apps/desktop/src/features/hoverbar/HoverbarDetailApp.tsx`
  - 保留查询、排序、刷新、主题、打开主窗口、收起和测高逻辑。
  - 继续保持一个平台渲染一个 `HoverbarPlatformCard`。
- `apps/desktop/src/styles/global.css`
  - 移除最右竖排状态条；改为卡头和账户行中的横向文字徽章。
  - 使用 `data-edge` 分别适配 top/bottom 与 left/right，禁止通过缩小到不可读字号解决宽度。
- `apps/desktop/src/features/hoverbar/HoverbarPreview.tsx`
  - 补充 GPT Plus + Free 双账户、GLM Token Plan + 个人余额、正常/部分可用/异常预览状态。
- `apps/desktop/src/features/hoverbar/hoverbar-state.ts`
  - 保留过滤、排序和头部聚合状态；不要把“部分可用”固定等同于缓存数据。
- `apps/desktop/src-tauri/src/windows/hoverbar.rs`
  - 默认不改窗口生命周期与定位；只有内容确实无法在现有 420/300 宽度适配时才调整逻辑尺寸，并同步前端 `measureHoverbar`。

## 验收标准

- 顶部、底部、左侧、右侧均一行一个平台，无双列平台、遮挡、溢出或文字裁切。
- GPT 两个及以上账户只占一张平台卡，账户数据与 Source 一一对应，不串值。
- GLM 等双能力平台的 Token Plan 与个人余额位于同一卡片。
- 5 小时和 7 天窗口均展示真实重置时间；缺失时不伪造。
- 悬浮详情中搜索不到 Credits、赠送额度、充值额度、缓存命中、Token 明细、今日消费和本月消费文案。
- 正常、部分可用、异常同时有文字和颜色；stale/missing 语义保持。
- 主窗口与悬浮详情继续共享同一份平台 ViewModel，不新增平台请求。
- 只执行与改动相称的 typecheck、build；若修改 Rust 窗口尺寸，再执行 `cargo check`。不要补大量测试。

## GPT 多账户与重置信号补充稿

- `12-hoverbar-gpt-multi-account-radar-summary.png`：顶部/底部最终稿。把平台状态固定在卡头右侧，Plus/Free 固定在对应账户行；完整展示每个账户的 5 小时、7 天窗口和时间，时间前不再显示“重置”。保持窄版比例，不横向拉长。
- `13-hoverbar-gpt-radar-detail.png`：点击摘要条后在悬浮面板内切换二级详情，展示来源摘要、最近三条 Tibo 中文动态和可选 AI 辅助分析；雷达结论始终标记为推测。
- `14-hoverbar-detail-right-docked-final.png`：右侧停靠最终稿。面板贴右侧并向屏幕内侧展开，小球始终贴在屏幕最右边；左侧停靠时镜像处理。
- 雷达来源状态、GPT 平台聚合状态和账户套餐类型属于三个不同语义，不共用一个徽章。
- Tibo 中文翻译需要后续增加可缓存字段；没有真实翻译时不得由前端临时伪造。
