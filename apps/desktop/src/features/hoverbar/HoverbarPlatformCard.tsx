/**
 * 悬浮详情平台卡片（最终稿）。
 * 一个平台一张卡：卡头为图标 + 名称 + 平台聚合状态；GPT/Codex 多账户同卡分组，
 * 套餐徽章跟随账户名；窗口时间只显示时间值；GPT 卡底部为重置信号摘要条。
 */
import { AlertTriangle, CheckCircle2, ChevronRight, CircleX, Radar, RefreshCw } from "lucide-react";
import type {
  CapabilitySnapshotViewModel,
  DataFreshness,
  PlatformAggregateStatus,
  PlatformSummaryViewModel,
  SourceState,
  SourceSummaryViewModel,
} from "@/lib/types";
import type { RadarSnapshot } from "@/lib/ipc";
import { compactPercentText } from "@/lib/format";
import { radarConfidenceLabel, radarSourceLine } from "./hoverbar-state";
import { HOVERBAR_PROVIDER_VISUALS } from "./provider-visuals";

const CORE_IDS = ["quota_window_5h", "quota_window_7d", "balance"] as const;
const DEEPSEEK_EXTRA_IDS = ["today_spend", "month_spend", "cache_hit_rate"] as const;
const ALLOWED_IDS = new Set<string>([...CORE_IDS, "plan_level", ...DEEPSEEK_EXTRA_IDS]);
const ACCOUNT_PROVIDERS = new Set(["openai", "claude_code"]);
const END_ALIGNED_IDS = new Set<string>(DEEPSEEK_EXTRA_IDS);

type AccountStatus = "healthy" | "partial" | "error";
type MetricAlign = "center" | "end";

type HoverbarMetric = {
  id: string;
  label: string;
  value: string | null;
  time: string | null;
  freshness: DataFreshness;
  align: MetricAlign;
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

const METRIC_LABEL: Record<string, string> = {
  quota_window_5h: "5小时窗口",
  quota_window_7d: "7天窗口",
  balance: "个人余额",
  today_spend: "今日消费",
  month_spend: "本月消费",
  cache_hit_rate: "缓存命中率",
};

function metricIdsFor(providerId: string): string[] {
  return providerId === "deepseek" ? [...CORE_IDS, ...DEEPSEEK_EXTRA_IDS] : [...CORE_IDS];
}

export function HoverbarPlatformCard({
  platform,
  radar,
  onOpenRadar,
  onRefreshRadar,
  radarRefreshing = false,
  radarRefreshError = null,
}: {
  platform: PlatformSummaryViewModel;
  radar?: RadarSnapshot;
  onOpenRadar?: () => void;
  onRefreshRadar?: () => void;
  radarRefreshing?: boolean;
  radarRefreshError?: string | null;
}) {
  const groups = buildGroups(platform);
  const multi = ACCOUNT_PROVIDERS.has(platform.providerId) && groups.length > 1;
  const single = groups[0];
  const hasStale = groups.some((group) => group.metrics.some((metric) => metric.freshness === "stale"));
  const visual = HOVERBAR_PROVIDER_VISUALS[platform.providerId];
  const showRadarStrip = platform.providerId === "openai" && radar !== undefined && onOpenRadar !== undefined;
  return (
    <article
      className="hb-service-card"
      data-status={platform.aggregateStatus}
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
        <b className="hb-card-name">{platform.displayName}</b>
        {!multi && single?.plan ? (
          <span className="hb-plan-chip" data-plan={planKey(single.plan)}>
            {single.plan}
          </span>
        ) : null}
        <StatusChip status={platform.aggregateStatus} />
      </header>

      {groups.length === 0 ? (
        <p className="hb-primary-missing">暂无真实数据</p>
      ) : multi ? (
        <div className="hb-groups">
          {groups.map((group) => (
            <section key={group.id} className="hb-group">
              <div className="hb-group-head">
                {group.plan ? (
                  <span className="hb-plan-chip" data-plan={planKey(group.plan)}>
                    {group.plan}
                  </span>
                ) : null}
                <span className="hb-group-name">{group.title}</span>
                {group.status !== "healthy" ? <StatusChip status={group.status} /> : null}
              </div>
              <MetricRows metrics={group.metrics} />
            </section>
          ))}
        </div>
      ) : (
        <MetricRows metrics={single?.metrics ?? []} />
      )}

      {showRadarStrip && radar && onOpenRadar ? (
        <RadarStrip
          radar={radar}
          onOpenRadar={onOpenRadar}
          onRefreshRadar={onRefreshRadar}
          radarRefreshing={radarRefreshing}
          radarRefreshError={radarRefreshError}
        />
      ) : null}
    </article>
  );
}

function StatusChip({ status }: { status: AccountStatus | PlatformAggregateStatus }) {
  const Icon = status === "healthy" ? CheckCircle2 : status === "error" ? CircleX : AlertTriangle;
  return (
    <span className="hb-status-chip" data-status={status}>
      <Icon size={11} aria-hidden />
      {STATUS_LABEL[status]}
    </span>
  );
}

function MetricRows({ metrics }: { metrics: HoverbarMetric[] }) {
  if (metrics.length === 0) {
    return <p className="hb-primary-missing">暂不可用</p>;
  }
  return (
    <div className="hb-rows">
      {metrics.map((metric) => {
        const missing = metric.value === null;
        return (
          <div
            key={metric.id}
            className="hb-row"
            data-id={metric.id}
            data-align={metric.align}
            data-freshness={metric.freshness}
            data-missing={missing || undefined}
          >
            <span className="hb-row-label">{metric.label}</span>
            {metric.align === "center" ? (
              <b className="hb-row-value" data-selectable="true">
                {missing ? "暂不可用" : metric.value}
              </b>
            ) : (
              <span className="hb-row-mid" />
            )}
            <span className="hb-row-end">
              {metric.align === "end" ? (
                <b className="hb-row-value" data-selectable="true">
                  {missing ? "暂不可用" : metric.value}
                </b>
              ) : null}
              {metric.time && !missing ? (
                <span className="hb-row-time" data-selectable="true">
                  {metric.time}
                </span>
              ) : null}
              {metric.freshness === "stale" && !missing ? (
                <span className="hb-row-stale">可能过期</span>
              ) : null}
            </span>
          </div>
        );
      })}
    </div>
  );
}

/** GPT 卡底部重置信号摘要条：与平台额度状态完全独立的雷达层。 */
function RadarStrip({
  radar,
  onOpenRadar,
  onRefreshRadar,
  radarRefreshing,
  radarRefreshError,
}: {
  radar: RadarSnapshot;
  onOpenRadar: () => void;
  onRefreshRadar?: () => void;
  radarRefreshing: boolean;
  radarRefreshError: string | null;
}) {
  const analysis = radar.analysis;
  const latest = radar.latest;
  const summary =
    analysis?.conclusion ??
    radar.notice?.headline ??
    latest?.summary ??
    latest?.translatedText ??
    latest?.text ??
    "暂未同步重置信号来源";
  const confidence = analysis?.errorMessage ? null : analysis?.confidence ?? null;
  const analyzeError = analysis?.errorMessage ?? radarRefreshError;
  return (
    <footer className="hb-radar-strip">
      <div className="hb-radar-strip-head">
        <Radar size={14} aria-hidden />
        <span className="hb-radar-strip-title">重置信号</span>
        {confidence ? (
          <span className="hb-radar-strip-confidence" data-level={confidence}>
            {radarConfidenceLabel(confidence)}把握
          </span>
        ) : null}
        <div className="hb-radar-strip-actions">
          {onRefreshRadar ? (
            <button
              type="button"
              className="hb-radar-strip-refresh"
              onClick={onRefreshRadar}
              disabled={radarRefreshing}
              data-loading={radarRefreshing || undefined}
              aria-label={radarRefreshing ? "正在同步重置信号" : "刷新重置信号"}
              title={radarRefreshing ? "正在同步…" : "刷新重置信号"}
            >
              <RefreshCw size={13} aria-hidden />
            </button>
          ) : null}
          <button type="button" className="hb-radar-strip-link" onClick={onOpenRadar}>
            查看详情
            <ChevronRight size={13} aria-hidden />
          </button>
        </div>
      </div>
      <p className="hb-radar-strip-summary" data-selectable="true">
        {radarRefreshing ? "正在同步 CodexRadar…" : summary}
      </p>
      {analyzeError ? <p className="hb-radar-strip-error">{analyzeError}</p> : null}
      <p className="hb-radar-strip-note">{radarSourceLine(radar)} · 仅为推测，不代表官方结论</p>
    </footer>
  );
}

function planKey(plan: string): string {
  const key = plan.trim().toLowerCase();
  return key === "plus" || key === "pro" || key === "free" ? key : "other";
}

function buildGroups(platform: PlatformSummaryViewModel): HoverbarGroup[] {
  const ids = metricIdsFor(platform.providerId);
  const configured = platform.sources.filter((source) => source.credentialConfigured);
  const groups = configured
    .map((source) => groupFromSource(source, capsFor(platform.capabilities, source.sourceId), ids))
    .filter((group): group is HoverbarGroup => group !== null);

  if (ACCOUNT_PROVIDERS.has(platform.providerId) && groups.length > 1) {
    return groups;
  }
  if (groups.length <= 1) return groups;
  return [mergeGroups(groups, ids)];
}

function groupFromSource(
  source: SourceSummaryViewModel,
  capabilities: CapabilitySnapshotViewModel[],
  ids: string[],
): HoverbarGroup | null {
  const metrics = ids
    .map((id) => toMetric(capabilities, id))
    .filter((metric): metric is HoverbarMetric => metric !== null);
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

function mergeGroups(groups: HoverbarGroup[], ids: string[]): HoverbarGroup {
  const metrics = ids
    .map((id) => groups.flatMap((group) => group.metrics).find((metric) => metric.id === id))
    .filter((metric): metric is HoverbarMetric => Boolean(metric));
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

function toMetric(capabilities: CapabilitySnapshotViewModel[], id: string): HoverbarMetric | null {
  const capability = capabilities.find((item) => item.capabilityId === id);
  if (!capability) return null;
  const raw =
    capability.freshness === "missing" || !capability.value.primary ? null : capability.value.primary;
  const value = raw ? compactPercentText(raw) : null;
  return {
    id,
    label: METRIC_LABEL[id] ?? capability.displayName,
    value,
    time: value ? extractWindowTime(capability.value.secondary) : null,
    freshness: capability.freshness,
    align: END_ALIGNED_IDS.has(id) ? "end" : "center",
  };
}

function planOf(capabilities: CapabilitySnapshotViewModel[]): string | null {
  const plan = capabilities.find((item) => item.capabilityId === "plan_level");
  const value = plan?.value.primary?.trim();
  return value || null;
}

/** 从 secondary 的「重置 14:30 / 重置于 09/02 08:00」片段取出纯时间值。 */
function extractWindowTime(secondary: string | null | undefined): string | null {
  if (!secondary) return null;
  const hit = secondary
    .split("·")
    .map((part) => part.trim())
    .find((part) => part.startsWith("重置") && part.length > 2);
  if (!hit) return null;
  const time = hit.replace(/^重置于?\s*[:：]?\s*/, "").trim();
  return time || null;
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
  if (source.sourceId === "openai-codex-local") return "本机";
  if (source.sourceId.startsWith("openai-codex-extra-")) {
    return source.displayName.replace(/^额外 ChatGPT 账号\s*/, "账号 ") || "额外账号";
  }
  return source.displayName.replace(/（当前 CLI）$/, "") || source.displayName;
}
