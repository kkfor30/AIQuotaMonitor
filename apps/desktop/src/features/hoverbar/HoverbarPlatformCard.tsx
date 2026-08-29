/**
 * 悬浮详情平台卡片。一个平台一张卡；GPT 多账户在卡内分组；
 * 只展示 5 小时/7 天窗口、重置时间、个人余额、plan_level 和状态。
 */
import { AlertTriangle, CheckCircle2, CircleX, Monitor, User } from "lucide-react";
import type {
  CapabilitySnapshotViewModel,
  DataFreshness,
  PlatformAggregateStatus,
  PlatformSummaryViewModel,
  SourceState,
  SourceSummaryViewModel,
} from "@/lib/types";
import { HOVERBAR_PROVIDER_VISUALS } from "./provider-visuals";

const ALLOWED_IDS = new Set(["quota_window_5h", "quota_window_7d", "balance", "plan_level"]);
const DATA_IDS = ["quota_window_5h", "quota_window_7d", "balance"] as const;
const ACCOUNT_PROVIDERS = new Set(["openai", "claude_code"]);

type AccountStatus = "healthy" | "partial" | "error";

type HoverbarMetric = {
  id: string;
  label: string;
  value: string | null;
  reset: string | null;
  freshness: DataFreshness;
};

type HoverbarGroup = {
  id: string;
  title: string;
  plan: string | null;
  status: AccountStatus;
  metrics: HoverbarMetric[];
  localAccount: boolean;
};

const STATUS_LABEL: Record<AccountStatus | PlatformAggregateStatus, string> = {
  healthy: "正常",
  partial: "部分可用",
  setup_required: "待配置",
  error: "异常",
};

const METRIC_LABEL: Record<(typeof DATA_IDS)[number], string> = {
  quota_window_5h: "5小时窗口",
  quota_window_7d: "7天窗口",
  balance: "个人余额",
};

export function HoverbarPlatformCard({ platform }: { platform: PlatformSummaryViewModel }) {
  const groups = buildGroups(platform);
  const multi = ACCOUNT_PROVIDERS.has(platform.providerId) && groups.length > 1;
  const single = groups[0];
  const hasStale = groups.some((group) => group.metrics.some((metric) => metric.freshness === "stale"));
  const visual = HOVERBAR_PROVIDER_VISUALS[platform.providerId];
  const platformStatus = platform.aggregateStatus;

  return (
    <article
      className="hb-service-card"
      data-status={platformStatus}
      data-freshness={hasStale ? "stale" : "fresh"}
      data-layout={multi ? "accounts" : "single"}
    >
      <header className="hb-card-head">
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
        <div className="hb-card-head-main">
          <b className="hb-card-name">{platform.displayName}</b>
          <div className="hb-card-meta">
            {multi ? <span className="hb-chip">{groups.length} 个账户</span> : null}
            {!multi && single?.plan ? <span className="hb-chip hb-chip-plan">{single.plan}</span> : null}
            <StatusChip status={platformStatus} />
          </div>
        </div>
      </header>

      {groups.length === 0 ? (
        <p className="hb-primary-missing">暂无真实数据</p>
      ) : multi ? (
        <div className="hb-groups">
          {groups.map((group) => (
            <section key={group.id} className="hb-group">
              <div className="hb-group-head">
                <span className="hb-group-icon" aria-hidden="true">
                  {group.localAccount ? <Monitor size={14} /> : <User size={14} />}
                </span>
                <span className="hb-group-name">{group.title}</span>
                {group.plan ? <span className="hb-chip hb-chip-plan">{group.plan}</span> : null}
                <StatusChip status={group.status} />
              </div>
              <MetricRow metrics={group.metrics} />
            </section>
          ))}
        </div>
      ) : (
        <MetricRow metrics={single?.metrics ?? []} />
      )}
    </article>
  );
}

function StatusChip({ status }: { status: AccountStatus | PlatformAggregateStatus }) {
  const Icon = status === "healthy" ? CheckCircle2 : status === "error" ? CircleX : AlertTriangle;
  return (
    <span className="hb-chip hb-chip-status" data-status={status}>
      <Icon size={11} aria-hidden />
      {STATUS_LABEL[status]}
    </span>
  );
}

function MetricRow({ metrics }: { metrics: HoverbarMetric[] }) {
  if (metrics.length === 0) {
    return <p className="hb-primary-missing">暂不可用</p>;
  }
  return (
    <div className="hb-metrics" data-count={metrics.length}>
      {metrics.map((metric) => {
        const missing = metric.value === null;
        return (
          <div
            key={metric.id}
            className="hb-metric"
            data-id={metric.id}
            data-freshness={metric.freshness}
            data-missing={missing || undefined}
          >
            <span className="hb-metric-label">{metric.label}</span>
            <b className="hb-metric-value">{missing ? "暂不可用" : metric.value}</b>
            {metric.reset ? <span className="hb-metric-reset">{metric.reset}</span> : null}
            {metric.freshness === "stale" && !missing ? (
              <span className="hb-metric-reset">可能过期</span>
            ) : null}
          </div>
        );
      })}
    </div>
  );
}

function buildGroups(platform: PlatformSummaryViewModel): HoverbarGroup[] {
  const configured = platform.sources.filter((source) => source.credentialConfigured);
  const groups = configured
    .map((source) => groupFromSource(source, capsFor(platform.capabilities, source.sourceId)))
    .filter((group): group is HoverbarGroup => group !== null);

  if (ACCOUNT_PROVIDERS.has(platform.providerId) && groups.length > 1) {
    return groups;
  }
  if (groups.length <= 1) return groups;
  return [mergeGroups(groups)];
}

function groupFromSource(
  source: SourceSummaryViewModel,
  capabilities: CapabilitySnapshotViewModel[],
): HoverbarGroup | null {
  const metrics = DATA_IDS.map((id) => toMetric(capabilities, id)).filter(
    (metric): metric is HoverbarMetric => metric !== null,
  );
  const plan = planOf(capabilities);
  if (metrics.length === 0 && !plan) return null;
  return {
    id: source.sourceId,
    title: accountTitle(source),
    plan,
    status: sourceStatus(source.state, metrics),
    metrics,
    localAccount: source.sourceId === "openai-codex-local",
  };
}

function mergeGroups(groups: HoverbarGroup[]): HoverbarGroup {
  const metrics = DATA_IDS.map((id) => groups.flatMap((group) => group.metrics).find((metric) => metric.id === id)).filter(
    (metric): metric is HoverbarMetric => Boolean(metric),
  );
  const plan = groups.map((group) => group.plan).find((value) => Boolean(value)) ?? null;
  const status = mergeStatus(groups.map((group) => group.status));
  return {
    id: groups.map((group) => group.id).join("+"),
    title: groups[0]?.title ?? "",
    plan,
    status,
    metrics,
    localAccount: false,
  };
}

function capsFor(capabilities: CapabilitySnapshotViewModel[], sourceId: string) {
  return capabilities.filter((item) => item.sourceId === sourceId && ALLOWED_IDS.has(item.capabilityId));
}

function toMetric(capabilities: CapabilitySnapshotViewModel[], id: (typeof DATA_IDS)[number]): HoverbarMetric | null {
  const capability = capabilities.find((item) => item.capabilityId === id);
  if (!capability) return null;
  const value =
    capability.freshness === "missing" || !capability.value.primary ? null : capability.value.primary;
  return {
    id,
    label: METRIC_LABEL[id],
    value,
    reset: value ? extractReset(capability.value.secondary) : null,
    freshness: capability.freshness,
  };
}

function planOf(capabilities: CapabilitySnapshotViewModel[]): string | null {
  const plan = capabilities.find((item) => item.capabilityId === "plan_level");
  const value = plan?.value.primary?.trim();
  return value || null;
}

function extractReset(secondary: string | null | undefined): string | null {
  if (!secondary) return null;
  const hit = secondary
    .split("·")
    .map((part) => part.trim())
    .find((part) => part.startsWith("重置") && part.length > 2);
  return hit ?? null;
}

function sourceStatus(state: SourceState, metrics: HoverbarMetric[]): AccountStatus {
  const usable = metrics.filter((metric) => metric.value !== null);
  if (state === "auth_required" || state === "error" || usable.length === 0) return "error";
  const complete = metrics.length > 0 && metrics.every((metric) => metric.freshness === "fresh" && metric.value);
  if (complete && (state === "ready" || state === "refreshing")) return "healthy";
  return "partial";
}

function mergeStatus(statuses: AccountStatus[]): AccountStatus {
  if (statuses.every((status) => status === "healthy")) return "healthy";
  if (statuses.every((status) => status === "error")) return "error";
  return "partial";
}

function accountTitle(source: SourceSummaryViewModel): string {
  if (source.sourceId === "openai-codex-local") return "本机账户";
  if (source.sourceId.startsWith("openai-codex-extra-")) {
    return source.displayName.replace(/^额外 ChatGPT 账号\s*/, "账号 ") || "额外账号";
  }
  return source.displayName.replace(/（当前 CLI）$/, "") || source.displayName;
}
