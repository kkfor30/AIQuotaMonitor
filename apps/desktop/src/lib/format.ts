/** 展示层格式化工具（金额已由后端格式化，此处仅处理时间与文本）。 */

/** 去掉百分比里无意义的 `.0`，例如 `40.0%` → `40%`，保留 `62.5%`。 */
export function compactPercentText(value: string): string {
  return value.replace(/(\d+)\.0(?=%)/g, "$1");
}

export function formatTime(epochMs: number | null | undefined): string {
  if (!epochMs) return "—";
  const date = new Date(epochMs);
  const now = new Date();
  const sameDay = date.toDateString() === now.toDateString();
  const hhmm = date.toLocaleTimeString("zh-CN", {
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  });
  if (sameDay) {
    const diffMin = Math.floor((now.getTime() - epochMs) / 60000);
    if (diffMin < 1) return "刚刚";
    if (diffMin < 60) return `${diffMin} 分钟前`;
    return `今天 ${hhmm}`;
  }
  const md = date.toLocaleDateString("zh-CN", { month: "2-digit", day: "2-digit" });
  return `${md} ${hhmm}`;
}

export function formatDateTime(epochMs: number | null | undefined): string {
  if (!epochMs) return "—";
  const date = new Date(epochMs);
  return date.toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  });
}
