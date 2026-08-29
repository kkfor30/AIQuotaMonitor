# AIQuotaMonitor Apple Glass V6 UI

本目录是主窗口高保真视觉重构基准。目标是提升材质、层级、对比度、品牌一致性和数据扫读效率，不改变现有平台能力、接入流程、雷达逻辑和设置含义。

## 设计稿

| 文件 | 页面 |
| --- | --- |
| `00-brand-logo-style.png` | 品牌、Logo 与视觉令牌 |
| `logo-aiquota-liquid-glass.png` | 新应用 Logo 资产 |
| `01-overview.png` | 总览：横向平台窗口、双趋势图、关注项、刷新记录 |
| `02-platform-center-quota.png` | 平台中心：GLM 额度与用量代表页 |
| `03-platform-center-access.png` | 平台中心：DeepSeek 接入与来源代表页 |
| `04-gpt-radar-summary.png` | GPT 重置雷达：信号摘要 |
| `05-gpt-radar-tibo.png` | GPT 重置雷达：Tibo 动态 |
| `06-gpt-radar-ai.png` | GPT 重置雷达：AI 辅助分析 |
| `07-settings-general.png` | 设置：常规与外观 |
| `08-settings-sort.png` | 设置：悬浮球排序 |
| `09-settings-refresh.png` | 设置：自动刷新 |
| `10-settings-data-about.png` | 设置：数据与关于 |

## 视觉原则

- 冷白到淡蓝画布，半透明白卡、细冷色边框和克制阴影。
- 主色 `#0A66FF`，成功色 `#00A67D`；橙色只用于真实警告，红色只用于真实错误或危险确认。
- 正文使用高对比深海军蓝，禁止为了“高级感”把辅助文字做得过浅。
- 使用 Windows 系统字体栈，优先 `Segoe UI Variable` / `Segoe UI`，不新增在线字体。
- 圆角、描边、阴影、控件高度和间距由统一 CSS token 管理。
- 玻璃只服务层级，不使用大面积强模糊、霓虹描边或过曝高光。

## 不可改变的业务边界

- 现有导航、Tab、表单字段、按钮、来源状态、刷新记录、雷达筛选、AI 开关、设置项及其行为全部保留。
- 平台接口、Rust Source adapter、Credential Manager、SQLite 快照、刷新协调器和 stale/missing/error 语义不得为了 UI 改变。
- PNG 数字只作视觉示例；实现继续消费真实 ViewModel，不补零、不造数。
- PNG 文字若与当前代码或产品文档不一致，以代码和产品文档为准。
- 平台必须使用官方 Logo，优先复用 `apps/desktop/public/assets/providers/`，不得使用字母占位或自行重绘。

## 总览唯一允许的结构改造

- “关键平台”改成固定高度横向窗口，支持滚轮/触控板横向浏览、拖拽滚动、前后箭头和位置提示。
- 支持手动拖拽平台卡排序，并复用现有平台顺序持久化语义；平台增多时不再向下堆叠。
- “消费趋势”和“窗口压力趋势”各自独立成折线图卡片。
- 消费趋势只纳入具有真实金额快照的平台；窗口压力只纳入具有 Token Plan 窗口能力的平台。每个平台颜色稳定、图例明确。
- 图表只使用真实历史快照；无数据时显示空状态，不生成演示曲线。

## Logo 使用

- 应用 Logo 使用 `logo-aiquota-liquid-glass.png`，用于侧栏品牌位和应用图标派生。
- 平台 Logo 与应用 Logo 完全分离；GPT/Codex、DeepSeek、GLM、Kimi 等使用各自官方资产。

