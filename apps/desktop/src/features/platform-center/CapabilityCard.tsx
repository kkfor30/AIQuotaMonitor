import { Banknote, Coins, Flame, Gauge, TrendingUp, Wallet } from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { FreshnessTag } from "@/components/ui/StatusBadge";
import { compactPercentText, formatTime } from "@/lib/format";
import type { CapabilitySnapshotViewModel } from "@/lib/types";

/** 能力卡片的图标映射（按能力 id；未知能力用仪表盘图标）。 */
const CAPABILITY_ICON: Record<string, LucideIcon> = {
  balance: Wallet,
  today_spend: Coins,
  month_spend: Banknote,
  model_usage_v4_flash: Flame,
  model_usage_v4_pro: Flame,
  cache_hit_rate: Gauge,
  cache_hit_tokens: Gauge,
  cache_miss_tokens: Gauge,
  prompt_tokens: Gauge,
  response_tokens: Gauge,
  quota_window_5h: Gauge,
  quota_window_7d: Gauge,
  quota_window_30d: Gauge,
  plan_level: Gauge,
  usage_trend: TrendingUp,
};

/**
 * 单个能力快照卡片（Apple Glass V6 平台中心）：
 * 图标 + 名称 + 新鲜度徽章 → 大号数值 → 次要说明与采集时间 → 用量进度条。
 * 进度条按剩余量口径配色：>50% 绿、10%~50% 蓝、<10% 红；stale 恒橙并标注最后成功时间。
 */
export function CapabilityCard({ capability }: { capability: CapabilitySnapshotViewModel }) {
  const Icon = CAPABILITY_ICON[capability.capabilityId] ?? Gauge;
  const isTrend = capability.value.kind === "trend";
  const progress = Math.min(1, Math.max(0, capability.value.progress ?? 0));
  const barColor =
    capability.freshness === "stale"
      ? "bg-q-warning"
      : progress > 0.5
        ? "bg-q-success"
        : progress >= 0.1
          ? "bg-q-primary"
          : "bg-q-danger";

  return (
    <div className="glass-panel flex flex-col gap-2.5 p-4">
      <div className="flex items-start justify-between gap-2">
        <div className="flex min-w-0 flex-1 items-start gap-2">
          <Icon size={16} aria-hidden className="mt-[3px] shrink-0 text-q-text-muted" />
          <span className="min-w-0 flex-1 break-words text-[13px] font-medium leading-[18px] text-q-text-secondary">
            {capability.displayName}
          </span>
        </div>
        <FreshnessTag freshness={capability.freshness} />
      </div>

      {capability.value.primary !== null ? (
        <p
          className="text-[28px] font-bold leading-9 tracking-tight tabular-nums text-q-text-primary"
          data-selectable="true"
        >
          {compactPercentText(capability.value.primary)}
        </p>
      ) : isTrend ? (
        <p className="text-[13px] text-q-text-muted">趋势见下方图表</p>
      ) : capability.value.secondary ? (
        <div>
          <p className="text-[13px] text-q-text-muted">暂无统计值</p>
          <p className="mt-1 text-xs text-q-text-muted">{compactPercentText(capability.value.secondary)}</p>
        </div>
      ) : (
        <p className="text-[13px] text-q-text-muted">暂无数据，待接入后展示</p>
      )}

      {capability.value.primary !== null && (capability.value.secondary || capability.capturedAt !== null) && (
        <div className="flex flex-col gap-0.5">
          {capability.value.secondary && (
            <p className="break-words text-[11px] leading-4 text-q-text-muted" title={capability.value.secondary}>
              {compactPercentText(capability.value.secondary)}
            </p>
          )}
          {capability.capturedAt !== null && (
            <p className="text-[11px] leading-4 text-q-text-muted">{formatTime(capability.capturedAt)}</p>
          )}
        </div>
      )}

      {capability.value.progress !== null && (
        <div
          className="h-1.5 overflow-hidden rounded-full bg-q-primary-softer"
          role="progressbar"
          aria-valuenow={Math.round(progress * 100)}
          aria-valuemin={0}
          aria-valuemax={100}
        >
          <div className={`h-full rounded-full ${barColor}`} style={{ width: `${progress * 100}%` }} />
        </div>
      )}

      {capability.freshness === "stale" && capability.lastGoodAt !== null && (
        <p className="text-[11px] text-q-warning">上次成功：{formatTime(capability.lastGoodAt)}</p>
      )}
    </div>
  );
}
