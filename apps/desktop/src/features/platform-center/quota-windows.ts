import type { CapabilitySnapshotViewModel } from "@/lib/types";

/** 窗口能力固定排序：通用 5h → 7d → 30d → Antigravity Gemini 5h → Gemini 周 → Claude 5h → Claude 周 → 其他。 */
export const WINDOW_ORDER = [
  "quota_window_5h",
  "quota_window_7d",
  "quota_window_30d",
  "quota_window_7d_opus",
  "quota_window_7d_sonnet",
  "quota_window_5h_gemini",
  "quota_window_7d_gemini",
  "quota_window_5h_3p",
  "quota_window_7d_3p",
];

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
  if (capability.capabilityId === "quota_window_5h_gemini") return "Gemini 5h";
  if (capability.capabilityId === "quota_window_7d_gemini") return "Gemini 周";
  if (capability.capabilityId === "quota_window_5h_3p") return "Claude 5h";
  if (capability.capabilityId === "quota_window_7d_3p") return "Claude 周";
  const name = capability.displayName.split("·").at(-1)?.trim() || capability.displayName;
  return name.replace(/窗口$/, "").trim();
}

/** 趋势折线稳定配色：按窗口类型固定，不随阈值变化（阈值颜色只用于剩余进度条）。 */
export const WINDOW_TREND_COLORS: Record<string, string> = {
  quota_window_5h: "#E5484D",
  quota_window_7d: "#0A66FF",
  quota_window_30d: "#00A67D",
  quota_window_5h_gemini: "#1a73e8",
  quota_window_7d_gemini: "#0d9488",
  quota_window_5h_3p: "#d97706",
  quota_window_7d_3p: "#8b5cf6",
};

export function windowTrendColor(capabilityId: string): string {
  return WINDOW_TREND_COLORS[capabilityId] ?? "#7C3AED";
}
