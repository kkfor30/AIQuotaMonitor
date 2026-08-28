/**
 * 悬浮详情平台卡片。
 *
 * 视觉结构迁移自 DeepSeek-Monitor-Windows/DeepSeekMonitorWindows
 * src/main.tsx 的 HoverbarDetailRow（提交 af6cfe07，MIT）。
 * 数据已改为当前 Source/Capability 脱敏 ViewModel，不读取旧版原始响应。
 */
import { AlertTriangle, CheckCircle2, CircleX, Settings2 } from "lucide-react";
import type {
  CapabilitySnapshotViewModel,
  PlatformAggregateStatus,
  PlatformSummaryViewModel,
} from "@/lib/types";
import { HOVERBAR_PROVIDER_VISUALS } from "./provider-visuals";

const PRIMARY_CAPABILITY_ORDER = [
  "balance",
  "quota_window_5h",
  "window_usage",
  "credits",
  "today_spend",
  "month_spend",
];

export function HoverbarPlatformCard({ platform }: { platform: PlatformSummaryViewModel }) {
  const visibleCapabilities = platform.capabilities.filter(
    (capability) => capability.value.primary !== null,
  );
  const primaryCapability = pickPrimaryCapability(visibleCapabilities);
  const supportingCapabilities = visibleCapabilities
    .filter(
      (capability) =>
        capability.capabilityId !== primaryCapability?.capabilityId &&
        capability.value.kind !== "trend",
    )
    .slice(0, 2);
  const problemSource =
    platform.aggregateStatus === "setup_required"
      ? undefined
      : platform.sources.find(
          (source) => source.state === "error" || source.state === "auth_required",
        );
  const hasStale = visibleCapabilities.some((capability) => capability.freshness === "stale");
  const visual = HOVERBAR_PROVIDER_VISUALS[platform.providerId];

  return (
    <article
      className="hb-service-card"
      data-status={platform.aggregateStatus}
      data-freshness={hasStale ? "stale" : primaryCapability?.freshness ?? "missing"}
    >
      <span className="hb-provider-logo" aria-hidden="true">
        {visual ? (
          <img
            src={visual.src}
            alt=""
            draggable={false}
            style={{ transform: `scale(${visual.scale})` }}
          />
        ) : (
          <span>{platform.displayName.slice(0, 1).toUpperCase()}</span>
        )}
      </span>

      <div className="hb-service-main">
        <div className="hb-service-title">
          <b>{platform.displayName}</b>
          <HoverbarStatus status={platform.aggregateStatus} />
        </div>
        {primaryCapability ? (
          <>
            <strong className="hb-primary-value" data-selectable="true">
              {primaryCapability.value.primary}
            </strong>
            <span className="hb-primary-label">{primaryCapability.displayName}</span>
          </>
        ) : (
          <p className="hb-primary-missing">
            {platform.aggregateStatus === "setup_required" ? "尚未接入" : "暂无真实数据"}
          </p>
        )}
      </div>

      <div className="hb-supporting-values">
        {supportingCapabilities.length > 0 ? (
          supportingCapabilities.map((capability) => (
            <span key={`${capability.sourceId}-${capability.capabilityId}`}>
              <i>{capability.displayName}</i>
              <b data-selectable="true">{capability.value.primary}</b>
            </span>
          ))
        ) : (
          <span>
            <i>接入方式</i>
            <b>{platform.accessSummary}</b>
          </span>
        )}
      </div>

      {(problemSource || hasStale) && (
        <div className="hb-card-message">
          <AlertTriangle size={12} aria-hidden />
          <span>
            {problemSource?.errorMessage ??
              (hasStale ? "部分能力正在使用上次成功快照" : "需要处理接入状态")}
          </span>
        </div>
      )}
    </article>
  );
}

function pickPrimaryCapability(
  capabilities: CapabilitySnapshotViewModel[],
): CapabilitySnapshotViewModel | undefined {
  for (const capabilityId of PRIMARY_CAPABILITY_ORDER) {
    const capability = capabilities.find((item) => item.capabilityId === capabilityId);
    if (capability) return capability;
  }
  return capabilities[0];
}

function HoverbarStatus({ status }: { status: PlatformAggregateStatus }) {
  const meta = {
    healthy: { label: "正常", icon: CheckCircle2 },
    partial: { label: "部分可用", icon: AlertTriangle },
    setup_required: { label: "待配置", icon: Settings2 },
    error: { label: "异常", icon: CircleX },
  }[status];
  const Icon = meta.icon;

  return (
    <span className="hb-status" data-status={status}>
      <Icon size={11} aria-hidden />
      {meta.label}
    </span>
  );
}
