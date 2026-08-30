import type { CapabilitySnapshotViewModel } from "@/lib/types";

/** 窗口能力固定排序：5 小时 → 7 天 → 30 天 → 其他真实窗口。 */
export const WINDOW_ORDER = ["quota_window_5h", "quota_window_7d", "quota_window_30d"];

export function windowOrder(id: string): number {
  const index = WINDOW_ORDER.indexOf(id);
  return index >= 0 ? index : WINDOW_ORDER.length;
}

export function sortWindowCapabilities(capabilities: CapabilitySnapshotViewModel[]): CapabilitySnapshotViewModel[] {
  return [...capabilities].sort(
    (left, right) =>
      windowOrder(left.capabilityId) - windowOrder(right.capabilityId)
      || left.capabilityId.localeCompare(right.capabilityId),
  );
}

export function windowShortLabel(capability: CapabilitySnapshotViewModel): string {
  if (capability.capabilityId === "quota_window_5h") return "5小时";
  if (capability.capabilityId === "quota_window_7d") return "7天";
  if (capability.capabilityId === "quota_window_30d") return "30天";
  const name = capability.displayName.split("·").at(-1)?.trim() || capability.displayName;
  return name.replace(/窗口$/, "");
}

/** 趋势折线稳定配色：按窗口类型固定，不随阈值变化（阈值颜色只用于剩余进度条）。 */
export const WINDOW_TREND_COLORS: Record<string, string> = {
  quota_window_5h: "#E5484D",
  quota_window_7d: "#0A66FF",
  quota_window_30d: "#00A67D",
};

export function windowTrendColor(capabilityId: string): string {
  return WINDOW_TREND_COLORS[capabilityId] ?? "#7C3AED";
}
