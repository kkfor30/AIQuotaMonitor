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

const PRIMARY_ORDER: Record<string, string[]> = {
  openai: ["quota_window_5h", "quota_window_7d", "plan_level"],
  deepseek: ["balance", "today_spend", "month_spend"],
};

const SUPPORTING_ORDER: Record<string, string[]> = {
  openai: ["quota_window_5h", "quota_window_7d", "credits", "plan_level"],
  deepseek: ["balance", "today_spend", "month_spend"],
};

const DEFAULT_PRIMARY_ORDER = ["balance", "quota_window_5h", "today_spend", "month_spend", "plan_level"];

const SHORT_LABEL: Record<string, string> = {
  balance: "账户余额",
  today_spend: "今日消费",
  month_spend: "本月消费",
  quota_window_5h: "5 小时窗口",
  quota_window_7d: "7 天窗口",
  credits: "Credits",
  plan_level: "订阅计划",
};

export function HoverbarPlatformCard({ platform }: { platform: PlatformSummaryViewModel }) {
  const visibleCapabilities = platform.capabilities.filter(
    (capability) => capability.value.primary !== null && capability.value.kind !== "trend",
  );
  const primaryCapability = pickPrimaryCapability(platform.providerId, visibleCapabilities);
  const supportingCapabilities = pickSupportingCapabilities(
    platform,
    visibleCapabilities,
    primaryCapability,
  );
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
          <strong className="hb-primary-value" data-selectable="true">
            {primaryCapability.value.primary}
          </strong>
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
              <i>{hoverbarCapabilityLabel(platform, capability)}</i>
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
  providerId: string,
  capabilities: CapabilitySnapshotViewModel[],
): CapabilitySnapshotViewModel | undefined {
  const order = PRIMARY_ORDER[providerId] ?? DEFAULT_PRIMARY_ORDER;
  for (const capabilityId of order) {
    const capability = capabilities.find((item) => item.capabilityId === capabilityId);
    if (capability) return capability;
  }
  return capabilities.find((item) => item.capabilityId !== "credits") ?? capabilities[0];
}

function pickSupportingCapabilities(
  platform: PlatformSummaryViewModel,
  capabilities: CapabilitySnapshotViewModel[],
  primary: CapabilitySnapshotViewModel | undefined,
): CapabilitySnapshotViewModel[] {
  const order = SUPPORTING_ORDER[platform.providerId];
  if (!order) {
    return capabilities.slice(0, 3);
  }
  const preferredSourceId = primary?.sourceId;
  const sameSource = preferredSourceId
    ? capabilities.filter((item) => item.sourceId === preferredSourceId)
    : capabilities;
  const rows: CapabilitySnapshotViewModel[] = [];
  for (const capabilityId of order) {
    const capability = sameSource.find((item) => item.capabilityId === capabilityId);
    if (capability) rows.push(capability);
  }
  for (const capability of capabilities) {
    if (rows.includes(capability)) continue;
    if (order.includes(capability.capabilityId)) rows.push(capability);
  }
  return rows;
}

function hoverbarCapabilityLabel(
  platform: PlatformSummaryViewModel,
  capability: CapabilitySnapshotViewModel,
): string {
  const short =
    SHORT_LABEL[capability.capabilityId] ?? capability.displayName.replace(/^.*·\s*/, "");
  const sameKind = platform.capabilities.filter(
    (item) => item.capabilityId === capability.capabilityId && item.value.primary !== null,
  ).length;
  if (sameKind <= 1) return short;
  const source = platform.sources.find((item) => item.sourceId === capability.sourceId);
  const sourceName = source?.displayName.replace(/（当前 CLI）$/, "") ?? "账号";
  return `${sourceName} · ${short}`;
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
