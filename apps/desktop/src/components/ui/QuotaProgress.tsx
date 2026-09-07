import { compactPercentText } from "@/lib/format";
import type { CapabilitySnapshotViewModel } from "@/lib/types";

export type QuotaTone = "good" | "mid" | "low" | "missing";

/** 三段色（V2 固定值，主题间一致）；悬浮卡等场景直接复用，不另起判断。 */
export const QUOTA_TONE_COLOR: Record<Exclude<QuotaTone, "missing">, string> = {
  good: "var(--q-quota-good)",
  mid: "var(--q-quota-mid)",
  low: "var(--q-quota-low)",
};

export function quotaToneColor(tone: QuotaTone): string | undefined {
  return tone === "missing" ? undefined : QUOTA_TONE_COLOR[tone];
}

/**
 * V7 统一剩余额度色阶（所有额度进度条共用同一判断）：
 * ≥80 绿、20~80 蓝、<20 红（80 归绿、20 归蓝）；无法取值归 missing 灰。
 */
export function quotaTone(remainingPercent: number | null | undefined): QuotaTone {
  if (remainingPercent === null || remainingPercent === undefined || !Number.isFinite(remainingPercent)) {
    return "missing";
  }
  if (remainingPercent >= 80) return "good";
  if (remainingPercent >= 20) return "mid";
  return "low";
}

/** 从能力快照推导剩余百分比：优先后端 progress（0-1 换算为 0-100），若已是百分比数值直接使用，否则解析 "62.5%" 形态主值。 */
export function capabilityRemainingPercent(capability: CapabilitySnapshotViewModel): number | null {
  if (capability.value.progress !== null) {
    return capability.value.progress <= 1.0 ? capability.value.progress * 100 : capability.value.progress;
  }
  if (capability.value.primary === null) return null;
  const parsed = Number.parseFloat(capability.value.primary.replace("%", "").trim());
  return Number.isFinite(parsed) ? parsed : null;
}

/**
 * 剩余额度进度条（V7 全局规范）：
 * - 填充长度与颜色都表示剩余量：≥80 绿 / 20~80 蓝 / <20 红；stale 按真实剩余值着色。
 * - missing 显示灰色空轨道与「未获取」，禁止当成 0%。
 * - 百分比文字始终存在（tabular-nums），并带 progressbar aria 属性。
 * - 余额等无上限指标不使用本组件。
 */
export function QuotaProgress({
  capability,
  label,
  className,
}: {
  capability: CapabilitySnapshotViewModel;
  /** 行首窗口名称（如「5小时」）；不传则只渲染轨道 + 百分比。 */
  label?: string;
  className?: string;
}) {
  const remaining = capabilityRemainingPercent(capability);
  const missing = capability.freshness === "missing" || remaining === null;
  const tone = quotaTone(remaining);
  const color = quotaToneColor(tone);
  const percentText = missing ? "未获取" : compactPercentText(`${round1(remaining ?? 0)}%`);

  return (
    <div className={className ?? "flex min-w-0 items-center gap-2"}>
      {label && (
        <span className="w-[58px] shrink-0 text-[11px] text-q-text-muted whitespace-nowrap truncate" title={label}>
          {label}
        </span>
      )}
      <div
        className="h-1.5 min-w-0 flex-1 overflow-hidden rounded-full bg-[var(--q-quota-track)]"
        role="progressbar"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={missing ? undefined : Math.round(remaining ?? 0)}
        aria-valuetext={missing ? "未获取" : `剩余 ${percentText}`}
      >
        <div
          className="h-full rounded-full transition-[width] duration-300"
          style={{ width: `${missing ? 0 : Math.min(100, Math.max(0, remaining ?? 0))}%`, background: color }}
        />
      </div>
      <span
        className="w-11 shrink-0 text-right text-[12px] font-semibold leading-4 tabular-nums"
        style={missing ? { color: "var(--q-text-muted)", fontWeight: 400 } : { color }}
        data-selectable="true"
      >
        {percentText}
      </span>
    </div>
  );
}

function round1(value: number): number {
  return Math.round(value * 10) / 10;
}
