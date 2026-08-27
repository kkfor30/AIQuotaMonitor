/**
 * 悬浮球纯状态函数。
 *
 * 迁移来源：DeepSeekMonitorWindows-final/src/hoverbar-state.ts（提交 f3ab3ec6，MIT）
 * 变更：
 *   - sortHoverbarOutcomes 改造为 sortHoverbarPlatforms，输入从旧版 quota outcome
 *     改为 PlatformSummaryViewModel（新契约），排序算法保持一致
 *   - finalHoverbarRefreshStatus 重写为 summarizeHoverbarStatus：旧版基于
 *     errorKind 的刷新语义尚未接入，阶段一按聚合状态推导文案
 *   - 未迁移 latestSortSaveFailureResolution（排序保存竞态裁决）：
 *     阶段一没有排序编辑 UI，迁移即为死代码，待设置页排序编辑实现时一并迁移
 *   - 未迁移旧版 HoverbarRefreshOutcome 类型：其语义由 Source 级错误结构替代
 */

export type HoverbarViewState = "anchor" | "detail";
export type HoverbarEvent = "hover-delay-elapsed" | "activate" | "leave-delay-elapsed";
export type HoverbarMotionPhase = "anchor" | "opening" | "visible" | "closing";
export type HoverbarMotionEvent =
  | "request-open"
  | "window-expanded"
  | "request-close"
  | "cancel-close"
  | "exit-finished";
export type HoverbarEdge = "top" | "right" | "bottom" | "left";
export type HoverbarSortMode = "manual" | "smart";

import type { PlatformSummaryViewModel } from "@/lib/types";

/** 默认平台顺序（后续由设置页排序编辑持久化）。 */
export const DEFAULT_HOVERBAR_PROVIDER_ORDER = [
  "deepseek",
  "openai",
  "claude_code",
  "glm",
  "kimi",
  "mimo",
  "minimax",
] as const;

/** 智能排序中优先展示的「套餐制」平台（与旧项目语义一致）。 */
const SMART_PLAN_PROVIDERS = new Set(["openai", "claude_code", "glm", "minimax"]);

export type HoverbarAnchor = {
  edge: HoverbarEdge;
  ratio: number;
};

export const HOVERBAR_ENTER_DELAY_MS = 1000;
export const HOVERBAR_LEAVE_DELAY_MS = 800;
export const HOVERBAR_EXIT_ANIMATION_MS = 160;
/** 拖动结束后抑制悬停展开的时间窗口。 */
export const HOVERBAR_DRAG_SUPPRESS_MS = 350;

/** 详情面板开合动画相位状态机。 */
export function nextHoverbarMotionPhase(
  phase: HoverbarMotionPhase,
  event: HoverbarMotionEvent,
): HoverbarMotionPhase {
  if (event === "request-open" && phase === "anchor") return "opening";
  if (event === "window-expanded" && phase === "opening") return "visible";
  if (event === "request-close" && (phase === "opening" || phase === "visible")) return "closing";
  if (event === "cancel-close" && phase === "closing") return "visible";
  if (event === "exit-finished" && phase === "closing") return "anchor";
  return phase;
}

/** 容错解析后端下发的锚点：非法 edge 归 top、非法 ratio 归 0.5。 */
export function normalizeHoverbarAnchor(value: unknown): HoverbarAnchor {
  if (!value || typeof value !== "object") return { edge: "top", ratio: 0.5 };
  const candidate = value as { edge?: unknown; ratio?: unknown };
  const edge = ["top", "right", "bottom", "left"].includes(String(candidate.edge))
    ? (candidate.edge as HoverbarEdge)
    : "top";
  const rawRatio = typeof candidate.ratio === "number" ? candidate.ratio : Number.NaN;
  const ratio = Number.isFinite(rawRatio) && rawRatio >= 0 && rawRatio <= 1 ? rawRatio : 0.5;
  return { edge, ratio };
}

/** 前端内容测量（与 Rust 侧 windows::hoverbar::logical_size 镜像）。 */
export function measureHoverbar(
  edge: HoverbarEdge,
  state: HoverbarViewState,
  contentHeight: number,
): { width: number; height: number } {
  if (state === "anchor") return { width: 40, height: 40 };
  if (edge === "top" || edge === "bottom") {
    return { width: 420, height: Math.min(420, Math.max(180, contentHeight)) };
  }
  return { width: 300, height: Math.min(480, Math.max(180, contentHeight)) };
}

/** 悬停/离开/点击驱动的视图状态机。 */
export function nextHoverbarState(
  state: HoverbarViewState,
  event: HoverbarEvent,
): HoverbarViewState {
  if (event === "leave-delay-elapsed") return "anchor";
  if (state === "anchor" && (event === "hover-delay-elapsed" || event === "activate")) {
    return "detail";
  }
  return state;
}

/**
 * 平台卡片排序：
 * - manual：严格按 providerOrder 索引，表外平台排最后
 * - smart：套餐制平台优先成组，组内保持手动顺序
 */
export function sortHoverbarPlatforms<T extends { providerId: string }>(
  platforms: T[],
  providerOrder: readonly string[],
  mode: HoverbarSortMode,
): T[] {
  const ranks = new Map(providerOrder.map((providerId, index) => [providerId, index]));
  const rankOf = (providerId: string) => ranks.get(providerId) ?? Number.MAX_SAFE_INTEGER;
  return [...platforms].sort((left, right) => {
    if (mode === "smart") {
      const leftGroup = SMART_PLAN_PROVIDERS.has(left.providerId) ? 0 : 1;
      const rightGroup = SMART_PLAN_PROVIDERS.has(right.providerId) ? 0 : 1;
      if (leftGroup !== rightGroup) return leftGroup - rightGroup;
    }
    return rankOf(left.providerId) - rankOf(right.providerId);
  });
}

/** 悬浮球头部状态文案：成功/部分/失败同时用文字表达。 */
export function summarizeHoverbarStatus(
  platforms: PlatformSummaryViewModel[],
  updatedAt: number | null,
): string {
  const connected = platforms.filter((p) => p.aggregateStatus !== "setup_required");
  if (connected.length === 0) return "暂无可展示额度";

  const failed = connected.filter((p) => p.aggregateStatus === "error");
  const partial = connected.filter((p) => p.aggregateStatus === "partial");
  if (failed.length === connected.length) return "全部平台更新失败";
  if (failed.length > 0) {
    const first = failed[0];
    return failed.length === 1 ? `${first.displayName} 更新失败` : `${failed.length} 个平台更新失败`;
  }
  if (partial.length > 0) {
    const first = partial[0];
    return partial.length === 1
      ? `${first.displayName} 部分可用 · 缓存数据`
      : `${partial.length} 个平台部分可用`;
  }
  if (updatedAt === null) return "数据就绪";
  const time = new Date(updatedAt).toLocaleTimeString("zh-CN", {
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  });
  return `更新于 ${time}`;
}
