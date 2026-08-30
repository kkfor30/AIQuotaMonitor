import { Banknote, Coins, Flame, Gauge, TrendingUp, Wallet } from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { QuotaProgress } from "@/components/ui/QuotaProgress";
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
 * 图标 + 名称 + 新鲜度徽章 → 大号数值 → 次要说明与采集时间 → 剩余额度进度条。
 * 进度条使用全局 V7 色阶（≥80 绿 / 20~80 蓝 / <20 红，80 归绿 20 归蓝）；
 * stale 按真实剩余值着色并标注最后成功时间；missing 窗口显示灰色空轨道「未获取」。
 */
export function CapabilityCard({ capability }: { capability: CapabilitySnapshotViewModel }) {
  const Icon = CAPABILITY_ICON[capability.capabilityId] ?? Gauge;
  const isTrend = capability.value.kind === "trend";
  const hasProgressBar =
    capability.value.progress !== null
    || (capability.capabilityId.startsWith("quota_window_") && capability.value.primary !== null);
  const showMissingTrack =
    !hasProgressBar && capability.freshness === "missing" && capability.capabilityId.startsWith("quota_window_");
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

      {(hasProgressBar || showMissingTrack) && <QuotaProgress capability={capability} />}

      {capability.freshness === "stale" && capability.lastGoodAt !== null && (
        <p className="text-[11px] text-q-warning">上次成功：{formatTime(capability.lastGoodAt)}</p>
      )}
    </div>
  );
}
