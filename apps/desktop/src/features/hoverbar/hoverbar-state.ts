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
import type { RadarSnapshot } from "@/lib/ipc";

/** 默认平台顺序（后续由设置页排序编辑持久化）。 */
export const DEFAULT_HOVERBAR_PROVIDER_ORDER = [
  "deepseek",
  "openai",
  "claude_code",
  "glm",
  "kimi",
  "mimo",
  "minimax",
  "siliconflow",
  "siliconflow_intl",
  "stepfun",
  "openrouter",
  "novita",
  "grok",
] as const;

/** 智能排序中优先展示的「套餐制」平台（与旧项目语义一致）。 */
const SMART_PLAN_PROVIDERS = new Set(["openai", "claude_code", "glm", "minimax", "grok"]);

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

/** 悬浮详情只展示已配置凭据的平台，未接入的留在平台中心。 */
export function filterHoverbarPlatforms<T extends { aggregateStatus: string }>(platforms: T[]): T[] {
  return platforms.filter((platform) => platform.aggregateStatus !== "setup_required");
}

/** 悬浮详情时间展示：当天只显示 HH:mm，跨天补 MM-DD 前缀。 */
export function formatHoverbarClock(value: number): string {
  const date = new Date(value);
  const now = new Date();
  const pad = (input: number) => String(input).padStart(2, "0");
  const hhmm = `${pad(date.getHours())}:${pad(date.getMinutes())}`;
  const sameDay =
    date.getFullYear() === now.getFullYear() &&
    date.getMonth() === now.getMonth() &&
    date.getDate() === now.getDate();
  return sameDay ? hhmm : `${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${hhmm}`;
}

/** 摘要条第二行：只保留更新时间，不附加来源站名。 */
export function radarSourceLine(radar: RadarSnapshot): string {
  if (radar.lastSyncedAt && radar.sourceStatus === "stale") {
    return `更新 ${formatHoverbarClock(radar.lastSyncedAt)} · 缓存可能过期`;
  }
  if (radar.lastSyncedAt) return `更新 ${formatHoverbarClock(radar.lastSyncedAt)}`;
  return "尚未同步";
}

/** 重置事件阶段中文标签；未知阶段不臆造文案。 */
export function radarPhaseLabel(phase: string | null | undefined): string | null {
  switch (phase) {
    case "watching":
      return "观察中";
    case "upcoming":
      return "即将重置";
    case "landed_claimed":
      return "来源称已落地";
    case "landed_observed":
      return "本机观察到刷新";
    case "closed":
      return "已结束";
    default:
      return null;
  }
}

/** 摘要条 AI 状态行：关闭/失败/待分析时不得用历史结论正文替代来源行。 */
export function radarAiLine(radar: RadarSnapshot): string {
  const ai = radar.aiAssessment;
  if (!ai.enabled) {
    return ai.history ? `已关闭 · 历史 ${formatHoverbarClock(ai.history.createdAt)}` : "已关闭";
  }
  switch (ai.state) {
    case "pending":
      return "有新动态待分析";
    case "failed":
      return ai.history ? `分析失败 · 历史 ${formatHoverbarClock(ai.history.createdAt)}` : "分析失败";
    case "current":
      return ai.current?.conclusion ?? "已分析";
    default:
      return "未分析";
  }
}

/** 本机额度验证状态中文文案。 */
export function quotaStatusText(status: string): string {
  switch (status) {
    case "unavailable":
      return "暂无法验证";
    case "insufficient_data":
      return "缺少基线";
    case "pending":
      return "待验证";
    case "scheduled":
      return "正常计划刷新";
    case "possible_reset":
      return "疑似刷新";
    case "unscheduled_reset":
      return "观察到非计划刷新";
    case "no_change":
      return "未见变化";
    default:
      return status;
  }
}

export /** 本机额度状态 + 归因的最终文案：时间与事件吻合或用户确认时，明确说「已重置」。 */
export function quotaStatusLabel(status: string, attribution: string): string {
  if (status === "unscheduled_reset") {
    if (attribution === "radar_correlated") return "已重置 · 本机已观察到";
    if (attribution === "user_confirmed") return "已重置 · 你已确认";
  }
  return quotaStatusText(status);
}

const QUOTA_STATUS_PRIORITY: string[] = [
  "unscheduled_reset",
  "possible_reset",
  "scheduled",
  "pending",
  "insufficient_data",
  "unavailable",
  "no_change",
];

/** 摘要条本机额度行：取最显著的验证状态；多账号状态不一致时带计数。 */
export function radarQuotaLine(radar: RadarSnapshot): string | null {
  const list = radar.quotaVerifications ?? [];
  if (list.length === 0) return null;
  const sorted = [...list].sort(
    (left, right) => QUOTA_STATUS_PRIORITY.indexOf(left.status) - QUOTA_STATUS_PRIORITY.indexOf(right.status),
  );
  const primary = sorted[0];
  const sameCount = list.filter((item) => item.status === primary.status).length;
  if (primary.status === "unscheduled_reset" && sameCount < list.length) {
    return `${sameCount}/${list.length} 账号观察到非计划刷新`;
  }
  return quotaStatusText(primary.status);
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
    if (partial.length > 1) return `${partial.length} 个平台部分可用`;
    const stale = first.capabilities.some((capability) => capability.freshness === "stale");
    return stale ? `${first.displayName} 部分可用 · 缓存可能过期` : `${first.displayName} 部分可用`;
  }
  if (updatedAt === null) return "数据就绪";
  const time = new Date(updatedAt).toLocaleTimeString("zh-CN", {
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  });
  return `更新于 ${time}`;
}
