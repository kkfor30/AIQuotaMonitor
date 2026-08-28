/**
 * 悬浮详情平台卡片。方案 A：一张卡多行对齐，状态贴最右侧竖排。
 */
import { AlertTriangle, CheckCircle2, CircleX, Settings2 } from "lucide-react";
import type {
  CapabilitySnapshotViewModel,
  PlatformSummaryViewModel,
  SourceAccessMode,
  SourceSummaryViewModel,
} from "@/lib/types";
import { HOVERBAR_PROVIDER_VISUALS } from "./provider-visuals";

const LANE_TYPE_LABEL: Record<SourceAccessMode, string> = {
  coding_plan: "套餐额度",
  token_plan: "套餐额度",
  personal_balance: "个人余额",
  web_usage: "网页用量",
  local_cli: "窗口",
};

const PLAN_MODES = new Set<SourceAccessMode>(["coding_plan", "token_plan"]);
const ACCOUNT_PROVIDERS = new Set(["openai", "claude_code"]);

type HoverbarLane = {
  id: string;
  typeLabel: string;
  capabilities: CapabilitySnapshotViewModel[];
};

export function HoverbarPlatformCard({ platform }: { platform: PlatformSummaryViewModel }) {
  const visibleCapabilities = platform.capabilities.filter(
    (capability) => capability.value.primary !== null && capability.value.kind !== "trend",
  );
  const lanes = buildHoverbarLanes(platform, visibleCapabilities);
  const problemSource =
    platform.aggregateStatus === "setup_required"
      ? undefined
      : platform.sources.find(
          (source) => source.credentialConfigured && (source.state === "error" || source.state === "auth_required"),
        );
  const hasStale = visibleCapabilities.some((capability) => capability.freshness === "stale");
  const visual = HOVERBAR_PROVIDER_VISUALS[platform.providerId];
  const status = STATUS_META[platform.aggregateStatus];
  const StatusIcon = status.icon;

  return (
    <article
      className="hb-service-card"
      data-status={platform.aggregateStatus}
      data-freshness={hasStale ? "stale" : "fresh"}
      data-lanes="rows"
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

      <div className="hb-card-body">
        <div className="hb-service-title">
          <b>{platform.displayName}</b>
        </div>
        {lanes.length > 0 ? (
          <div className="hb-lanes">
            {lanes.map((lane) => {
              const { primary, extra } = formatLane(lane);
              return (
                <div key={lane.id} className="hb-lane">
                  <i className="hb-lane-type">{lane.typeLabel}</i>
                  <b className="hb-lane-primary">{primary}</b>
                  <span className="hb-lane-extra">
                    {extra.map((line) => (
                      <span key={line}>{line}</span>
                    ))}
                  </span>
                </div>
              );
            })}
          </div>
        ) : (
          <p className="hb-primary-missing">
            {platform.aggregateStatus === "setup_required" ? "尚未接入" : "暂无真实数据"}
          </p>
        )}
      </div>

      <span className="hb-status" data-status={platform.aggregateStatus} title={status.label}>
        <StatusIcon size={11} aria-hidden />
        {status.label}
      </span>

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

const STATUS_META = {
  healthy: { label: "正常", icon: CheckCircle2 },
  partial: { label: "部分可用", icon: AlertTriangle },
  setup_required: { label: "待配置", icon: Settings2 },
  error: { label: "异常", icon: CircleX },
} as const;

function buildHoverbarLanes(
  platform: PlatformSummaryViewModel,
  visible: CapabilitySnapshotViewModel[],
): HoverbarLane[] {
  const configured = platform.sources.filter((source) => source.credentialConfigured);
  const plan = configured.filter((source) => PLAN_MODES.has(accessModeOf(source)));
  const balance = configured.filter((source) => accessModeOf(source) === "personal_balance");
  if (plan.length > 0 && balance.length > 0) {
    return [
      laneFromSources(visible, plan, LANE_TYPE_LABEL[accessModeOf(plan[0])]),
      laneFromSources(visible, balance, LANE_TYPE_LABEL.personal_balance),
    ].filter((lane) => lane.capabilities.length > 0);
  }

  if (ACCOUNT_PROVIDERS.has(platform.providerId) && configured.length > 0) {
    const lanes = configured
      .map((source) =>
        laneFromSources(visible, [source], accountLabel(source, configured.length > 1)),
      )
      .filter((lane) => lane.capabilities.length > 0);
    if (lanes.length > 0) return lanes;
  }

  const usage = configured.filter((source) => accessModeOf(source) === "web_usage");
  const money = configured.filter((source) => accessModeOf(source) === "personal_balance");
  if (usage.length > 0 && money.length > 0) {
    return [
      laneFromSources(visible, money, "余额"),
      laneFromSources(visible, usage, "用量"),
    ].filter((lane) => lane.capabilities.length > 0);
  }

  if (configured.length === 1) {
    const source = configured[0];
    const mode = accessModeOf(source);
    return [laneFromSources(visible, [source], LANE_TYPE_LABEL[mode] || "额度")].filter(
      (lane) => lane.capabilities.length > 0,
    );
  }

  if (visible.length === 0) return [];
  return [
    {
      id: "all",
      typeLabel: "额度",
      capabilities: visible,
    },
  ];
}

function laneFromSources(
  visible: CapabilitySnapshotViewModel[],
  sources: SourceSummaryViewModel[],
  typeLabel: string,
): HoverbarLane {
  const ids = new Set(sources.map((source) => source.sourceId));
  return {
    id: sources.map((source) => source.sourceId).join("+") || typeLabel,
    typeLabel,
    capabilities: visible.filter((capability) => ids.has(capability.sourceId)),
  };
}

function formatLane(lane: HoverbarLane): { primary: string; extra: string[] } {
  const byId = (id: string) => lane.capabilities.find((item) => item.capabilityId === id);
  const fiveHour = byId("quota_window_5h");
  const sevenDay = byId("quota_window_7d");
  const plan = formatPlan(byId("plan_level")?.value.primary);
  const credits = byId("credits")?.value.primary
    ? `Credits ${byId("credits")?.value.primary}`
    : null;
  const meta = [plan, credits].filter((item): item is string => Boolean(item));
  const balance = byId("balance");
  const today = byId("today_spend");
  const month = byId("month_spend");

  if (fiveHour?.value.primary) {
    return {
      primary: `5小时 ${fiveHour.value.primary}`,
      extra: [
        sevenDay?.value.primary ? `7天 ${sevenDay.value.primary}` : null,
        meta.join(" · ") || null,
      ].filter((item): item is string => Boolean(item)),
    };
  }

  if (sevenDay?.value.primary) {
    return { primary: `7天 ${sevenDay.value.primary}`, extra: meta };
  }

  if (balance?.value.primary) {
    return {
      primary: balance.value.primary,
      extra: [
        today?.value.primary ? `今日 ${today.value.primary}` : null,
        month?.value.primary ? `本月 ${month.value.primary}` : null,
        secondaryGift(balance.value.secondary),
      ].filter((item): item is string => Boolean(item)),
    };
  }

  const first = lane.capabilities[0];
  const extra = lane.capabilities
    .slice(1)
    .map((item) => item.value.primary)
    .filter((item): item is string => Boolean(item));
  return { primary: first?.value.primary ?? "—", extra };
}

function formatPlan(value: string | null | undefined): string | null {
  if (!value) return null;
  const trimmed = value.trim();
  if (!trimmed) return null;
  const normalized = trimmed.toLowerCase();
  if (/(^|[^a-z\u4e00-\u9fff])free([^a-z\u4e00-\u9fff]|$)|免费/.test(normalized)) return "Free";
  if (/(^|[^a-z\u4e00-\u9fff])plus([^a-z\u4e00-\u9fff]|$)/.test(normalized)) return "Plus";
  if (/(^|[^a-z\u4e00-\u9fff])pro([^a-z\u4e00-\u9fff]|$)/.test(normalized)) return "Pro";
  return trimmed.replace(/订阅|计划/g, "").trim() || trimmed;
}

function secondaryGift(value: string | null | undefined): string | null {
  if (!value || !/赠送|充值/.test(value)) return null;
  return value;
}

function accountLabel(source: SourceSummaryViewModel, multiple: boolean): string {
  if (source.sourceId === "openai-codex-local") return multiple ? "本机" : "窗口";
  if (source.sourceId.startsWith("openai-codex-extra-")) {
    return source.displayName.replace(/^额外 ChatGPT 账号\s*/, "账号 ") || "额外账号";
  }
  if (!multiple) return LANE_TYPE_LABEL[accessModeOf(source)] || "窗口";
  return source.displayName.replace(/（当前 CLI）$/, "") || "账号";
}

function accessModeOf(source: SourceSummaryViewModel): SourceAccessMode {
  return source.accessMode ?? "personal_balance";
}
