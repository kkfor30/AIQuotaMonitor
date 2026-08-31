/**
 * 悬浮详情平台卡片（最终稿）。
 * 一个平台一张卡：卡头为图标 + 名称 + 平台聚合状态；按后端 accounts 分组，
 * 每个账号只聚合自己的 Source 与 Capability；窗口时间只显示时间值；
 * GPT 卡底部为重置信号摘要条（只展示简短 conclusion）。
 */
import { AlertTriangle, CheckCircle2, ChevronRight, CircleX, Radar, RefreshCw } from "lucide-react";
import type {
  CapabilitySnapshotViewModel,
  DataFreshness,
  PlatformAggregateStatus,
  PlatformSummaryViewModel,
} from "@/lib/types";
import type { RadarSnapshot } from "@/lib/ipc";
import { compactPercentText } from "@/lib/format";
import {
  QUOTA_STATUS_PRIORITY,
  radarAiLine,
  radarPhaseLabel,
  radarQuotaLine,
  radarSourceLine,
} from "./hoverbar-state";
import { hoverbarProviderVisual } from "./provider-visuals";

const DEEPSEEK_EXTRA_IDS = ["today_spend", "month_spend", "cache_hit_rate"] as const;
const ALLOWED_IDS = new Set<string>(["balance", "plan_level", ...DEEPSEEK_EXTRA_IDS]);
const WINDOW_ORDER = ["quota_window_5h", "quota_window_7d", "quota_window_30d"];

type HoverbarMetric = {
  id: string;
  label: string;
  value: string | null;
  time: string | null;
  freshness: DataFreshness;
};

type HoverbarGroup = {
  id: string;
  title: string;
  plan: string | null;
  status: PlatformAggregateStatus;
  metrics: HoverbarMetric[];
};

const STATUS_LABEL: Record<PlatformAggregateStatus, string> = {
  healthy: "正常",
  partial: "部分可用",
  setup_required: "待配置",
  error: "异常",
};

const METRIC_LABEL: Record<string, string> = {
  quota_window_5h: "5小时窗口",
  quota_window_7d: "7天窗口",
  quota_window_30d: "30天窗口",
  balance: "个人余额",
  today_spend: "今日消费",
  month_spend: "本月消费",
  cache_hit_rate: "缓存命中率",
};

function metricIdsFor(providerId: string): string[] {
  return providerId === "deepseek" ? ["balance", ...DEEPSEEK_EXTRA_IDS] : ["balance"];
}

function isQuotaWindow(id: string): boolean {
  return id.startsWith("quota_window_");
}

function compareWindowIds(left: string, right: string): number {
  const leftIndex = WINDOW_ORDER.indexOf(left);
  const rightIndex = WINDOW_ORDER.indexOf(right);
  if (leftIndex >= 0 && rightIndex >= 0) return leftIndex - rightIndex;
  if (leftIndex >= 0) return -1;
  if (rightIndex >= 0) return 1;
  return left.localeCompare(right);
}

export function HoverbarPlatformCard({
  platform,
  radar,
  onOpenRadar,
  onRefreshRadar,
  onCancelRadar,
  radarRefreshing = false,
  radarRefreshError = null,
}: {
  platform: PlatformSummaryViewModel;
  radar?: RadarSnapshot;
  onOpenRadar?: () => void;
  onRefreshRadar?: () => void;
  onCancelRadar?: () => void;
  radarRefreshing?: boolean;
  radarRefreshError?: string | null;
}) {
  const groups = buildGroups(platform);
  const multi = platform.accounts.length > 1;
  const single = groups[0];
  const hasStale = groups.some((group) => group.metrics.some((metric) => metric.freshness === "stale"));
  const visual = hoverbarProviderVisual(platform.providerId);
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
          onCancelRadar={onCancelRadar}
          radarRefreshing={radarRefreshing}
          radarRefreshError={radarRefreshError}
        />
      ) : null}
    </article>
  );
}

function StatusChip({ status }: { status: PlatformAggregateStatus }) {
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
          <div key={metric.id} className="hb-row">
            <span className="hb-row-label">{metric.label}</span>
            <span
              className="hb-row-value"
              data-id={metric.id}
              data-selectable="true"
              data-freshness={metric.freshness}
              data-missing={missing || undefined}
            >
              {missing ? "暂不可用" : metric.value}
            </span>
            <span className="hb-row-end">
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
  onCancelRadar,
  radarRefreshing,
  radarRefreshError,
}: {
  radar: RadarSnapshot;
  onOpenRadar: () => void;
  onRefreshRadar?: () => void;
  onCancelRadar?: () => void;
  radarRefreshing: boolean;
  radarRefreshError: string | null;
}) {
  // 三路证据各行独立：CodexRadar 来源行不受 AI 开关影响，AI 关闭时历史正文不得替换来源行。
  // 来源行优先站点公告；站点无公告区块时回退最新帖摘要（保持信息可用）。
  const source =
    radar.sourceAssessment?.headline ??
    radar.notice?.headline ??
    radar.latest?.summary ??
    radar.latest?.translatedText ??
    radar.latest?.text ??
    "暂未同步来源内容";
  const phase = radarPhaseLabel(radar.event?.phase);
  const aiLine = radarAiLine(radar);
  const quotaLine = radarQuotaLine(radar);
  const quotaStatus = (radar.quotaVerifications ?? [])
    .map((item) => item.status)
    .sort(
      (left, right) =>
        QUOTA_STATUS_PRIORITY.indexOf(left) - QUOTA_STATUS_PRIORITY.indexOf(right),
    )[0];
  return (
    <footer className="hb-radar-strip">
      <div className="hb-radar-strip-head">
        <Radar size={14} aria-hidden />
        <span className="hb-radar-strip-title">重置雷达</span>
        {phase ? (
          <span className="radar-phase-badge" data-phase={radar.event?.phase}>
            {phase}
          </span>
        ) : null}
        <div className="hb-radar-strip-actions">
          {onRefreshRadar ? (
            <button
              type="button"
              className="hb-radar-strip-refresh"
              onClick={radarRefreshing && onCancelRadar ? onCancelRadar : onRefreshRadar}
              aria-label={radarRefreshing ? "终止检查" : "刷新重置信号"}
              title={radarRefreshing ? "终止检查" : "刷新重置信号"}
            >
              <RefreshCw size={13} aria-hidden className={radarRefreshing ? "hb-spin" : ""} />
            </button>
          ) : null}
          <button type="button" className="hb-radar-strip-link" onClick={onOpenRadar}>
            查看详情
            <ChevronRight size={13} aria-hidden />
          </button>
        </div>
      </div>
      <div className="hb-radar-strip-row">
        <span className="hb-radar-strip-tag">CodexRadar</span>
        <span className="hb-radar-strip-row-text" data-selectable="true">
          {radarRefreshing ? "正在同步 CodexRadar…" : source}
        </span>
      </div>
      <div className="hb-radar-strip-row">
        <span className="hb-radar-strip-tag">AI分析</span>
        <span className="hb-radar-strip-row-text" data-selectable="true">
          {aiLine}
        </span>
      </div>
      {quotaLine ? (
        <div className="hb-radar-strip-row">
          <span className="hb-radar-strip-tag">本机额度</span>
          <span className="hb-radar-strip-row-text" data-quota={quotaStatus} data-selectable="true">
            {quotaLine}
          </span>
        </div>
      ) : null}
      {radarRefreshError ? <p className="hb-radar-strip-error">{radarRefreshError}</p> : null}
      <p className="hb-radar-strip-note">{radarSourceLine(radar)} · 仅为推测，不代表官方结论</p>
    </footer>
  );
}

function planKey(plan: string): string {
  const key = plan.trim().toLowerCase();
  return key === "plus" || key === "pro" || key === "free" || key === "lite" ? key : "other";
}

/**
 * 按后端账号列表构建分组：每个账号只聚合自己的 Source 与 Capability，
 * 账号顺序沿用后端（本机 → 默认 → 额外）。未配置来源的能力不展示，不补零。
 */
function buildGroups(platform: PlatformSummaryViewModel): HoverbarGroup[] {
  const ids = metricIdsFor(platform.providerId);
  const multi = platform.accounts.length > 1;
  const configuredSourceIds = new Set(
    platform.sources.filter((source) => source.credentialConfigured).map((source) => source.sourceId),
  );
  const groups = platform.accounts.map((account) =>
    groupFromAccount(account, platform.capabilities, ids, configuredSourceIds),
  );
  if (!multi && groups.every((group) => group.metrics.length === 0 && !group.plan)) {
    return [];
  }
  return groups;
}

function groupFromAccount(
  account: PlatformSummaryViewModel["accounts"][number],
  capabilities: CapabilitySnapshotViewModel[],
  ids: string[],
  configuredSourceIds: Set<string>,
): HoverbarGroup {
  const own = capabilities.filter(
    (item) =>
      item.accountId === account.accountId &&
      configuredSourceIds.has(item.sourceId) &&
      (ALLOWED_IDS.has(item.capabilityId) || isQuotaWindow(item.capabilityId)),
  );
  const plan = planOf(own);
  const windowMetrics = own
    .filter((item) => isQuotaWindow(item.capabilityId) && hasWindowValue(item))
    .sort((left, right) => compareWindowIds(left.capabilityId, right.capabilityId))
    .map(capabilityToMetric);
  const extraMetrics = ids
    .map((id) => toMetric(own, id))
    .filter((metric): metric is HoverbarMetric => metric !== null);
  return {
    id: account.accountId,
    title: accountTitle(account),
    plan,
    status: account.status,
    metrics: [...windowMetrics, ...extraMetrics],
  };
}

function hasWindowValue(capability: CapabilitySnapshotViewModel): boolean {
  return capability.freshness !== "missing" && Boolean(capability.value.primary);
}

function capabilityToMetric(capability: CapabilitySnapshotViewModel): HoverbarMetric {
  const value = compactPercentText(capability.value.primary ?? "");
  return {
    id: capability.capabilityId,
    label: windowMetricLabel(capability),
    value,
    time: extractWindowTime(capability.value.secondary),
    freshness: capability.freshness,
  };
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
  };
}

function windowMetricLabel(capability: CapabilitySnapshotViewModel): string {
  if (METRIC_LABEL[capability.capabilityId]) return METRIC_LABEL[capability.capabilityId];
  const name = capability.displayName.split("·").at(-1)?.trim() || capability.displayName;
  return name;
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

function accountTitle(account: PlatformSummaryViewModel["accounts"][number]): string {
  if (account.kind === "local") return "本机";
  const name = account.displayName.trim();
  return name || "账号";
}
