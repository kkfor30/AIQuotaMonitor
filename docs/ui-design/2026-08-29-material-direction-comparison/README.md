# AIQuotaMonitor 材质方向对比

本目录用于确认主窗口与悬浮详情的最终皮肤。三套方案使用相同业务内容和布局，只比较画布、表面、玻璃厚度、对比度、边界与阴影，不代表新增功能。

## 设计稿

| 文件 | 方向 |
| --- | --- |
| `01-mica-main-window.png` | Windows Mica Premium 主窗口 |
| `02-mica-hoverbar.png` | Windows Mica Premium 右侧悬浮详情 |
| `03-graphite-main-window.png` | Graphite Glass Pro 主窗口 |
| `04-graphite-hoverbar.png` | Graphite Glass Pro 右侧悬浮详情 |
| `05-warm-ceramic-main-window.png` | Warm Ceramic Glass 主窗口 |
| `06-warm-ceramic-hoverbar.png` | Warm Ceramic Glass 右侧悬浮详情 |

## 方向判断

- **Windows Mica Premium**：推荐作为默认浅色主题。稳定灰蓝 Mica 画布，标题栏和侧栏是一层连续轻玻璃，内容卡接近实体，适合数据密集主窗口。
- **Graphite Glass Pro**：推荐作为深色主题。深石墨 Mica 基底、克制蓝绿高光，常驻桌面的悬浮详情质感最好，避免纯黑和赛博霓虹。
- **Warm Ceramic Glass**：可作为浅色备选。温暖珍珠灰与瓷釉表面更柔和，但不建议额外增加第三套主题，除非最终明确选它替代 Mica。

建议组合：**浅色使用 Mica，深色使用 Graphite；主窗口和悬浮详情共享同一套语义 Token，只让悬浮外壳使用更厚的 Acrylic。**

## 不可改变的内容

- 四项一级导航、标题栏和现有窗口控制不变。
- 总览继续保留整体状态、横向关键平台窗口、拖拽排序、需要关注、窗口压力趋势、消费趋势和最近刷新。
- 平台增多后在固定高度窗口内横向浏览，不能重新向下堆叠。
- 悬浮球继续使用现有 40×40 锚点、四边吸附、多显示器、全屏隐藏、延迟展开/收起与共享 ViewModel。
- 右侧停靠时小球必须位于物理屏幕最右侧，详情向左展开；其他方向镜像处理。
- 悬浮详情一平台一张卡；GPT 多账户同卡分组，GLM Token Plan 与个人余额同卡展示。
- 只展示真实窗口额度、时间、个人余额、套餐等级、平台/账户状态及 GPT 重置信号；缺失值不补零、不造数。
- GPT 重置信号保持推测属性，与平台额度状态、账户套餐状态分离。

## Logo 与实现说明

- 实现必须使用 `apps/desktop/public/assets/providers/` 中的平台资产；设计稿中的 Logo 只用于方向预览，不能从 PNG 裁切后投入生产。
- 应用 Logo 沿用 `docs/ui-design/2026-08-29-apple-glass-v6/logo-aiquota-liquid-glass.png`。
- 这些图片是宽画布视觉基准，不是生产窗口像素规格。主窗口与悬浮详情继续遵守现有响应式尺寸和测高逻辑。
- 本组图片使用内置 imagegen 串行生成，参考了 V6 总览、V5 右侧悬浮最终稿和项目内平台 Logo。

