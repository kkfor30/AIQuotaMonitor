import { Banknote, Coins, Flame, Gauge, TrendingUp, Wallet } from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { FreshnessTag } from "@/components/ui/StatusBadge";
import { formatTime } from "@/lib/format";
import type { CapabilitySnapshotViewModel } from "@/lib/types";

/** 能力卡片的图标映射（按能力 id；未知能力用仪表盘图标）。 */
const CAPABILITY_ICON: Record<string, LucideIcon> = {
  balance: Wallet,
  today_spend: Coins,
  month_spend: Banknote,
  model_usage_v4_flash: Flame,
  model_usage_v4_pro: Flame,
  cache_hit_rate: Gauge,
  usage_trend: TrendingUp,
};

/**
 * 单个能力快照卡片。
 * 状态表达规则：freshness 标签 + 文字；stale 必须显示最后成功时间；
 * missing 显示空态文案，禁止补零。
 */
export function CapabilityCard({ capability }: { capability: CapabilitySnapshotViewModel }) {
  const Icon = CAPABILITY_ICON[capability.capabilityId] ?? Gauge;
  const isTrend = capability.value.kind === "trend";

  return (
    <div className="glass-panel flex flex-col gap-3 p-4">
      <div className="flex items-center justify-between gap-2">
        <div className="flex items-center gap-2 text-q-text-secondary">
          <Icon size={15} aria-hidden />
          <span className="text-[13px] font-medium">{capability.displayName}</span>
        </div>
        <FreshnessTag freshness={capability.freshness} />
      </div>

      {capability.value.primary !== null ? (
        <div>
          <p
            className="text-[26px] font-semibold leading-none tracking-tight text-q-text-primary"
            data-selectable="true"
          >
            {capability.value.primary}
          </p>
          {capability.value.secondary && (
            <p className="mt-1.5 text-xs text-q-text-muted">{capability.value.secondary}</p>
          )}
        </div>
      ) : isTrend ? (
        <p className="text-xs text-q-text-muted">趋势见下方图表</p>
      ) : (
        <p className="text-[13px] text-q-text-muted">暂无数据，待接入后展示</p>
      )}

      {capability.value.progress !== null && (
        <div
          className="h-1.5 overflow-hidden rounded-full bg-q-primary-softer"
          role="progressbar"
          aria-valuenow={Math.round((capability.value.progress ?? 0) * 100)}
          aria-valuemin={0}
          aria-valuemax={100}
        >
          <div
            className={`h-full rounded-full ${
              capability.freshness === "stale" ? "bg-q-warning" : "bg-q-primary"
            }`}
            style={{ width: `${Math.min(100, Math.max(0, (capability.value.progress ?? 0) * 100))}%` }}
          />
        </div>
      )}

      {capability.freshness === "stale" && capability.lastGoodAt !== null && (
        <p className="text-[11px] text-q-warning">上次成功：{formatTime(capability.lastGoodAt)}</p>
      )}
    </div>
  );
}
