import { CapabilityCard } from "./CapabilityCard";
import { RefreshHistory } from "./RefreshHistory";
import { SourceHealthSummary } from "./SourceHealthSummary";
import { UsageTrend } from "./UsageTrend";
import { EmptyState } from "@/components/ui/EmptyState";
import type { PlatformSummaryViewModel } from "@/lib/types";

/**
 * 平台中心 / 额度与用量：
 * 来源健康摘要 + 能力卡片网格 + 趋势 + 刷新记录。
 * 未配置平台显示接入引导，不显示任何示例数值。
 */
export function UsageView({ platform }: { platform: PlatformSummaryViewModel }) {
  if (platform.aggregateStatus === "setup_required") {
    return (
      <EmptyState
        title={`${platform.displayName} 尚未接入`}
        description="配置数据来源后即可在此查看额度与用量。切换到「接入与来源」开始配置。"
      />
    );
  }

  const trendCapability = platform.capabilities.find(
    (capability) => capability.capabilityId === "usage_trend",
  );
  const cardCapabilities = platform.capabilities.filter(
    (capability) => capability.capabilityId !== "usage_trend",
  );

  return (
    <div className="grid min-h-0 flex-1 grid-cols-[minmax(0,1fr)_280px] gap-4 overflow-y-auto pr-1">
      <div className="flex min-w-0 flex-col gap-4">
        <SourceHealthSummary platform={platform} />
        <div className="grid grid-cols-2 gap-4 xl:grid-cols-3">
          {cardCapabilities.map((capability) => (
            <CapabilityCard key={capability.capabilityId} capability={capability} />
          ))}
        </div>
        {trendCapability && trendCapability.trend.length > 0 && (
          <UsageTrend capability={trendCapability} />
        )}
      </div>
      <RefreshHistory platform={platform} />
    </div>
  );
}
