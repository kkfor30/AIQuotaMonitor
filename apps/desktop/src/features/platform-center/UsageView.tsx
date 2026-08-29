import { CapabilityCard } from "./CapabilityCard";
import { RefreshHistory } from "./RefreshHistory";
import { SourceHealthSummary } from "./SourceHealthSummary";
import { UsageTrend } from "./UsageTrend";
import { EmptyState } from "@/components/ui/EmptyState";
import type { CapabilitySnapshotViewModel, PlatformSummaryViewModel } from "@/lib/types";

/** 能力语义分组：与呈现顺序无关的业务类别（纯展示层归类）。 */
function isVisibleQuotaCard(capability: CapabilitySnapshotViewModel): boolean {
  if (!capability.capabilityId.startsWith("quota_window_")) return true;
  return capability.freshness !== "missing" && Boolean(capability.value.primary);
}

function capabilitySection(capabilityId: string): { title: string; order: number } | null {
  if (capabilityId === "usage_trend") return null;
  if (capabilityId.startsWith("quota_window")) return { title: "窗口额度", order: 0 };
  if (capabilityId === "credits" || capabilityId === "plan_level") return { title: "订阅信息", order: 1 };
  if (capabilityId === "balance" || capabilityId === "month_spend" || capabilityId === "today_spend" || capabilityId === "total_spend") {
    return { title: "余额与消费", order: 2 };
  }
  return { title: "用量统计", order: 3 };
}

function groupedCapabilitySections(
  capabilities: CapabilitySnapshotViewModel[],
): Array<{ title: string; capabilityNames: string[]; capabilities: CapabilitySnapshotViewModel[] }> {
  const sections = new Map<string, { order: number; capabilities: CapabilitySnapshotViewModel[] }>();
  for (const capability of capabilities) {
    const section = capabilitySection(capability.capabilityId);
    if (!section) continue;
    const existing = sections.get(section.title);
    if (existing) {
      existing.capabilities.push(capability);
    } else {
      sections.set(section.title, { order: section.order, capabilities: [capability] });
    }
  }
  return [...sections.entries()]
    .sort((left, right) => left[1].order - right[1].order)
    .map(([title, { capabilities }]) => ({
      title,
      capabilityNames: capabilities.map((capability) => capability.displayName),
      capabilities,
    }));
}

/**
 * 平台中心 / 额度与用量（Apple Glass V6）：
 * 来源健康摘要 → 语义分组能力卡（窗口额度 / 订阅信息 / 余额与消费 / 用量统计）→
 * 消费趋势图 → 右侧最近刷新记录。未配置平台显示接入引导，不显示任何示例数值。
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
    (capability) => capability.capabilityId !== "usage_trend" && isVisibleQuotaCard(capability),
  );

  return (
    <div className="grid min-h-0 flex-1 grid-cols-[minmax(0,1fr)_300px] gap-4 overflow-y-auto pr-1">
      <div className="flex min-w-0 flex-col gap-4">
        <SourceHealthSummary platform={platform} />
        {groupedCapabilitySections(cardCapabilities).map((section) => (
          <section key={section.title} className="flex min-w-0 flex-col gap-3">
            <div className="flex flex-wrap items-baseline gap-x-2.5 gap-y-0.5 px-1">
              <h3 className="text-[14px] font-semibold tracking-tight text-q-text-primary">
                {section.title}
              </h3>
              <span className="break-words text-xs text-q-text-muted">
                {section.capabilityNames.join(" / ")}
              </span>
            </div>
            <div className="grid grid-cols-1 gap-4 xl:grid-cols-2 2xl:grid-cols-3">
              {section.capabilities.map((capability) => (
                <CapabilityCard
                  key={`${capability.sourceId}-${capability.capabilityId}`}
                  capability={capability}
                />
              ))}
            </div>
          </section>
        ))}
        {trendCapability && trendCapability.trend.length > 0 && (
          <UsageTrend capability={trendCapability} />
        )}
      </div>
      <RefreshHistory platform={platform} />
    </div>
  );
}
