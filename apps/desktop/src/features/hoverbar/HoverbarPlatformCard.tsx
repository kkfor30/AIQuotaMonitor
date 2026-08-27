/**
 * 悬浮详情平台卡片。
 * 与主窗口消费同一份 PlatformSummaryViewModel（get_platform_summaries），
 * 展示主能力数值与聚合状态；未配置平台只显示接入引导。
 */
import { PlatformMark } from "@/features/platform-center/ProviderRail";
import { AggregateStatusBadge, FreshnessTag } from "@/components/ui/StatusBadge";
import type { PlatformSummaryViewModel } from "@/lib/types";

export function HoverbarPlatformCard({ platform }: { platform: PlatformSummaryViewModel }) {
  const primaryCapability = platform.capabilities.find(
    (capability) => capability.value.primary !== null,
  );

  return (
    <article className="hb-card flex items-center gap-3 rounded-q-control border border-q-border bg-white/85 px-3 py-2.5 shadow-q-sm backdrop-blur-sm">
      <PlatformMark providerId={platform.providerId} size={32} />
      <div className="min-w-0 flex-1">
        <div className="flex items-center justify-between gap-2">
          <p className="truncate text-[13px] font-medium text-q-text-primary">
            {platform.displayName}
          </p>
          <AggregateStatusBadge status={platform.aggregateStatus} />
        </div>
        {primaryCapability ? (
          <div className="mt-1 flex items-baseline justify-between gap-2">
            <p className="truncate text-[15px] font-semibold tracking-tight text-q-text-primary" data-selectable="true">
              {primaryCapability.value.primary}
            </p>
            <span className="shrink-0 text-[11px] text-q-text-muted">
              {primaryCapability.displayName}
            </span>
          </div>
        ) : (
          <p className="mt-1 truncate text-[12px] text-q-text-muted">
            {platform.aggregateStatus === "setup_required" ? "尚未接入，前往平台中心配置" : "暂无可展示数据"}
          </p>
        )}
      </div>
      {primaryCapability && <FreshnessTag freshness={primaryCapability.freshness} />}
    </article>
  );
}
