/**
 * 悬浮详情平台卡片（Aurora Acrylic V2 认可稿 09/10 + DeepSeek 用量重设计 V2）。
 * 一个平台一张卡：卡头为图标 + 名称 + 平台聚合状态；套餐徽章与账户别名下沉为
 * 各账户分区的分组头行（首分区同构、无分隔线），别名过长只在本行内截断；
 * 每个账号只聚合自己的 Source 与 Capability；
 * 窗口行统一为「窗口名称 → 细进度条 → 剩余百分比 → 重置时间」（数值型 remainingPercent）；
 * 额外余额（credits）与可用重置卡按账号独立展示；资金组合组在四个停靠方向统一为
 * 余额独占行 + 今日/本月双胶囊的堆叠布局；
 * DeepSeek 模型行为 V4 Flash / V4 Flash Vision / V4 Pro 三条独立身份行
 * （晶体翼 / 光圈 / 神经旋涡图标方块 + 语义副标题，宽停靠「语义 · 本月 Token」、侧边只留语义）；
 * 模型列表后为总缓存命中率独立全宽块（靶心图标 + 细主蓝进度条 + 后端 secondary 说明），
 * 只消费总 cache_hit_rate，不展示分模型缓存/请求/Token，不套三段色阶；
 * stale 保留真实值与进度色，仅以低饱和蓝灰缓存提示；
 * GPT 卡底部为重置信号摘要条（只展示简短 conclusion）。
 */
import { AlertTriangle, CheckCircle2, ChevronRight, CircleDollarSign, CircleX, Radar, RefreshCw, TimerReset, Wallet } from "lucide-react";
import {
  FlashCrystalIcon,
  ProCoreIcon,
  TargetRingIcon,
  VisionApertureIcon,
} from "@/components/ui/MetricIcons";
import type {
  CapabilitySnapshotViewModel,
  DataFreshness,
  PlatformAggregateStatus,
  PlatformSummaryViewModel,
} from "@/lib/types";
import type { RadarSnapshot } from "@/lib/ipc";
import { compactPercentText } from "@/lib/format";
import { capabilityRemainingPercent, quotaTone, quotaToneColor } from "@/components/ui/QuotaProgress";
import {
  formatHoverbarClock,
  radarAiStripLine,
  radarBankedGrantLine,
  radarDecisionBadge,
  radarDecisionStripLine,
  radarDecisionStripLineCompact,
  radarSourceLine,
} from "./hoverbar-state";
import { hoverbarProviderVisual } from "./provider-visuals";

const DEEPSEEK_EXTRA_IDS = new Set<string>(["today_spend", "month_spend", "cache_hit_rate"]);
const DEEPSEEK_MODEL_ORDER = ["model_usage_v4_flash", "model_usage_v4_flash_vision", "model_usage_v4_pro"] as const;
const DEEPSEEK_MODEL_IDS = new Set<string>(DEEPSEEK_MODEL_ORDER);
const ALLOWED_IDS = new Set<string>(["balance", "total_spend", "credits", "banked_reset_count", "plan_level", "account_name", ...DEEPSEEK_EXTRA_IDS, ...DEEPSEEK_MODEL_IDS]);
const WINDOW_ORDER = [
  "quota_window_5h",
  "quota_window_7d",
  "quota_window_30d",
  "quota_window_7d_opus",
  "quota_window_7d_sonnet",
  "quota_window_5h_gemini",
  "quota_window_7d_gemini",
  "quota_window_5h_3p",
  "quota_window_7d_3p",
];

/**
 * 悬浮模型行身份元数据（重设计 V2）：独立身份图标 + 语义副标题。
 * 宽停靠（顶部/底部 420px）展示「语义 · 本月 Token」，侧边 300px 只保留语义部分（CSS 切换）。
 */
const MODEL_LINE_META: Record<string, { name: string; sub: string; variant: string }> = {
  model_usage_v4_flash: { name: "V4 Flash", sub: "旗舰轻量模型", variant: "flash" },
  model_usage_v4_flash_vision: { name: "V4 Flash Vision", sub: "视觉模型", variant: "vision" },
  model_usage_v4_pro: { name: "V4 Pro", sub: "深度思考模型", variant: "pro" },
};

/** 窗口额度行数据：percent 为数值型剩余百分比，色阶由 QuotaProgress 三段规则给出。 */
type HoverbarWindow = {
  id: string;
  label: string;
  percent: number | null;
  percentText: string | null;
  time: string | null;
  freshness: DataFreshness;
};

type HoverbarFinance = {
  value: string | null;
  freshness: DataFreshness;
};

type HoverbarCache = {
  percent: number | null;
  percentText: string | null;
  /** 后端 secondary 说明（如「命中 181.25M / 输入 234.52M」），前端不自行汇总。 */
  desc: string | null;
  freshness: DataFreshness;
};

type HoverbarModel = {
  id: string;
  value: string | null;
  freshness: DataFreshness;
};

type HoverbarSection = {
  id: string;
  title: string;
  plan: string | null;
  status: PlatformAggregateStatus;
  windows: HoverbarWindow[];
  bankedReset: HoverbarFinance | null;
  credits: HoverbarFinance | null;
  balance: HoverbarFinance | null;
  spend: { today: HoverbarFinance | null; month: HoverbarFinance | null; total?: HoverbarFinance | null };
  models: HoverbarModel[];
  cacheHit: HoverbarCache | null;
  /** 分区内存在 stale 快照时的低饱和缓存提示（带最后一次成功时间）；null 表示无 stale。 */
  staleNote: string | null;
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
  quota_window_7d_opus: "周窗口 · Opus",
  quota_window_7d_sonnet: "周窗口 · Sonnet",
  quota_window_5h_gemini: "Gemini 5h",
  quota_window_7d_gemini: "Gemini 周",
  quota_window_5h_3p: "Claude 5h",
  quota_window_7d_3p: "Claude 周",
};

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
  onRefreshSinglePlatform,
  singleRefreshing = false,
  onNavigateToPlatform,
  radar,
  onOpenRadar,
  onRefreshRadar,
  onCancelRadar,
  radarRefreshing = false,
  radarRefreshError = null,
}: {
  platform: PlatformSummaryViewModel;
  onRefreshSinglePlatform?: () => void;
  singleRefreshing?: boolean;
  onNavigateToPlatform?: (tab?: "usage" | "sources") => void;
  radar?: RadarSnapshot;
  onOpenRadar?: () => void;
  onRefreshRadar?: () => void;
  onCancelRadar?: () => void;
  radarRefreshing?: boolean;
  radarRefreshError?: string | null;
}) {
  const sections = buildSections(platform);
  const multi = platform.accounts.length > 1;
  const first = sections[0];
  const hasStale = sections.some((section) => section.staleNote !== null);
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
        <b className="hb-card-name" title={platform.displayName}>{platform.displayName}</b>
        <div className="hb-card-head-end">
          {onRefreshSinglePlatform && (
            <button
              type="button"
              className="hb-card-refresh-btn"
              aria-label={`刷新 ${platform.displayName} 额度`}
              title={singleRefreshing ? "正在刷新…" : `重新拉取 ${platform.displayName} 额度`}
              disabled={singleRefreshing}
              data-refreshing={singleRefreshing || undefined}
              onClick={(e) => {
                e.stopPropagation();
                onRefreshSinglePlatform();
              }}
            >
              <RefreshCw size={12} className={singleRefreshing ? "animate-spin text-q-primary" : ""} aria-hidden />
            </button>
          )}
          <StatusChip
            status={platform.aggregateStatus}
            interactive={Boolean(onNavigateToPlatform)}
            onClick={
              onNavigateToPlatform
                ? () =>
                    onNavigateToPlatform(
                      platform.aggregateStatus === "error" || platform.aggregateStatus === "setup_required"
                        ? "sources"
                        : "usage",
                    )
                : undefined
            }
          />
        </div>
      </header>

      {sections.length === 0 || !first ? (
        <p className="hb-primary-missing">暂无真实数据</p>
      ) : (
        <div className="hb-sections">
          {sections.map((section, index) => (
            <section
              key={section.id}
              className={index === 0 ? "hb-group hb-group-first" : "hb-group"}
            >
              <GroupHead section={section} showAlias={multi} showStatus={index > 0} />
              <SectionBody section={section} />
            </section>
          ))}
        </div>
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

function StatusChip({
  status,
  interactive = false,
  onClick,
}: {
  status: PlatformAggregateStatus;
  interactive?: boolean;
  onClick?: () => void;
}) {
  const Icon = status === "healthy" ? CheckCircle2 : status === "error" ? CircleX : AlertTriangle;
  const content = (
    <>
      <Icon size={11} aria-hidden />
      {STATUS_LABEL[status]}
    </>
  );

  if (interactive && onClick) {
    return (
      <button
        type="button"
        onClick={(e) => {
          e.stopPropagation();
          onClick();
        }}
        className="hb-status-chip cursor-pointer hover:brightness-110 active:scale-95 transition-all"
        data-status={status}
        data-interactive="true"
        title="点击在主窗口平台中心管理"
      >
        {content}
      </button>
    );
  }

  return (
    <span className="hb-status-chip" data-status={status}>
      {content}
    </span>
  );
}

/**
 * 账户分区头：套餐徽章 + 账户别名独占一行，别名过长只在本行内截断，不与平台名争抢空间。
 * 单账户且无套餐的平台（DeepSeek/Kimi/MiMo 等）不渲染此行，与认可稿一致；
 * 别名仅多账户平台展示；非首分区的状态异常徽章保留在此行（首分区由卡头聚合状态覆盖）。
 */
function GroupHead({
  section,
  showAlias,
  showStatus,
}: {
  section: HoverbarSection;
  showAlias: boolean;
  showStatus: boolean;
}) {
  if (!section.plan && !showAlias && (!showStatus || section.status === "healthy")) return null;
  return (
    <div className="hb-group-head">
      {section.plan ? (
        <span className="hb-plan-chip" data-plan={planKey(section.plan)}>
          {section.plan}
        </span>
      ) : null}
      {showAlias ? <span className="hb-group-name">{section.title}</span> : null}
      {showStatus && section.status !== "healthy" ? (
        <span className="hb-group-status">
          <StatusChip status={section.status} />
        </span>
      ) : null}
    </div>
  );
}

/** 一个账户分区的数据体：窗口额度行 → 可用重置卡 → 额外余额 → 资金组合 → 模型行 → 总缓存块 → 缓存提示。 */
function SectionBody({ section }: { section: HoverbarSection }) {
  const hasSpend =
    section.spend.today !== null || section.spend.month !== null || Boolean(section.spend.total);
  if (
    section.windows.length === 0
    && !section.bankedReset
    && !section.credits
    && !section.balance
    && !hasSpend
    && section.models.length === 0
    && !section.cacheHit
  ) {
    return <p className="hb-primary-missing">暂不可用</p>;
  }
  return (
    <div className="hb-section-body">
      {section.windows.map((item) => (
        <QuotaLine key={item.id} item={item} />
      ))}
      {(section.bankedReset || section.credits) && (
        <div className="hb-finance-capsules">
          {section.bankedReset ? <BankedResetBar item={section.bankedReset} /> : null}
          {section.credits ? <CreditsBar credits={section.credits} /> : null}
        </div>
      )}
      {(section.balance || hasSpend) && (
        /* 资金组合组：四个停靠方向统一堆叠布局（余额胶囊行 + 今日/本月双胶囊） */
        <div className="hb-finance-group">
          {section.balance ? <BalanceBar balance={section.balance} /> : null}
          {hasSpend ? (
            <div className="hb-spend-grid">
              {section.spend.today ? <SpendCell label="今日消费" item={section.spend.today} /> : null}
              {section.spend.month ? <SpendCell label="本月消费" item={section.spend.month} /> : null}
              {section.spend.total ? <SpendCell label="累计消费" item={section.spend.total} /> : null}
            </div>
          ) : null}
        </div>
      )}
      {section.models.length > 0 ? (
        <div className="hb-model-list">
          {section.models.map((model) => (
            <ModelLine key={model.id} model={model} />
          ))}
        </div>
      ) : null}
      {section.cacheHit ? <CacheLine cache={section.cacheHit} /> : null}
      {section.staleNote ? <p className="hb-stale-note">{section.staleNote}</p> : null}
    </div>
  );
}

/** 统一额度行：missing 显示灰轨道与「暂不可用」，不补零；stale 按真实剩余着色。 */
function QuotaLine({ item }: { item: HoverbarWindow }) {
  const missing = item.freshness === "missing" || item.percent === null;
  const tone = quotaTone(item.percent);
  const color = quotaToneColor(tone);
  return (
    <div className="hb-quota-line">
      <span className="hb-quota-label" title={item.label}>{item.label}</span>
      <span
        className="hb-quota-track"
        role="progressbar"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={missing ? undefined : Math.round(item.percent ?? 0)}
        aria-valuetext={missing ? "暂不可用" : `剩余 ${item.percentText}`}
      >
        <span
          className="hb-quota-fill"
          style={{
            width: `${missing ? 0 : Math.min(100, Math.max(0, item.percent ?? 0))}%`,
            background: color,
          }}
        />
      </span>
      <span
        className="hb-quota-percent"
        data-missing={missing || undefined}
        data-freshness={item.freshness}
        style={missing ? undefined : { color }}
        data-selectable="true"
      >
        {missing ? "暂不可用" : item.percentText}
      </span>
      <span className="hb-quota-time" data-selectable="true">
        {!missing && item.time ? item.time : null}
      </span>
    </div>
  );
}

/** 统一余额财务条：钱包线性图标 + 「个人余额」，金额 tabular 等宽、严格右对齐。 */
function BalanceBar({ balance }: { balance: HoverbarFinance }) {
  const missing = balance.freshness === "missing" || balance.value === null;
  return (
    <div className="hb-balance-bar">
      <Wallet size={14} aria-hidden />
      <span className="hb-balance-label">个人余额</span>
      <span
        className="hb-balance-amount"
        data-missing={missing || undefined}
        data-freshness={balance.freshness}
        data-selectable="true"
      >
        {missing ? "暂不可用" : balance.value}
      </span>
    </div>
  );
}

/** Codex 可用重置卡：真实 0 显示「0 张」，字段缺失显示暂不可用，不补零、不与额外余额换算。 */
function BankedResetBar({ item }: { item: HoverbarFinance }) {
  const missing = item.freshness === "missing" || item.value === null;
  return (
    <div className="hb-balance-bar hb-credits-bar">
      <TimerReset size={15} aria-hidden />
      <span className="hb-credits-label">可用重置卡</span>
      <span
        className="hb-credits-amount"
        data-missing={missing || undefined}
        data-freshness={item.freshness}
        data-selectable="true"
      >
        {missing ? "暂不可用" : `${item.value} 张`}
      </span>
    </div>
  );
}

/** Codex 额外余额：沿用财务条结构，但按账号独立归属，不与个人余额或其他账号合并。 */
function CreditsBar({ credits }: { credits: HoverbarFinance }) {
  const missing = credits.freshness === "missing" || credits.value === null;
  return (
    <div className="hb-balance-bar hb-credits-bar">
      <CircleDollarSign size={15} aria-hidden />
      <span className="hb-credits-label">额外余额</span>
      <span
        className="hb-credits-amount"
        data-missing={missing || undefined}
        data-freshness={credits.freshness}
        data-selectable="true"
      >
        {missing ? "暂不可用" : credits.value}
      </span>
    </div>
  );
}

function SpendCell({ label, item }: { label: string; item: HoverbarFinance }) {
  const missing = item.freshness === "missing" || item.value === null;
  return (
    <div className="hb-spend-cell">
      <span className="hb-spend-name">{label}</span>
      <span
        className="hb-spend-value"
        data-missing={missing || undefined}
        data-freshness={item.freshness}
        data-selectable="true"
      >
        {missing ? "暂不可用" : item.value}
      </span>
    </div>
  );
}

/**
 * 总缓存命中率块（DeepSeek 用量重设计 V2）：模型列表后的独立全宽块，
 * 靶心数据环图标 + 固定主色细进度条 + 百分比 + 后端 secondary 说明；
 * 不套剩余额度三段色阶（非额度语义），不并入任何模型行。
 */
function CacheLine({ cache }: { cache: HoverbarCache }) {
  const missing = cache.freshness === "missing" || cache.percentText === null;
  return (
    <div className="hb-cache-block">
      <div className="hb-cache-head">
        <TargetRingIcon size={24} />
        <span className="hb-cache-label">
          总缓存命中率<span className="hb-cache-scope">（全部模型）</span>
        </span>
        <span className="hb-cache-value" data-missing={missing || undefined} data-selectable="true">
          {missing ? "暂不可用" : cache.percentText}
        </span>
      </div>
      <span
        className="hb-quota-track"
        role="progressbar"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={missing ? undefined : Math.round(cache.percent ?? 0)}
      >
        <span
          className="hb-quota-fill"
          style={{
            width: `${missing ? 0 : Math.min(100, Math.max(0, cache.percent ?? 0))}%`,
            background: "var(--q-hoverbar-primary)",
          }}
        />
      </span>
      {!missing && cache.desc ? <p className="hb-cache-desc">{cache.desc}</p> : null}
    </div>
  );
}

/** V4 Flash / Vision / Pro 模型行：独立身份图标方块 + 语义副标题 + 右对齐真实 Token 文本。 */
function ModelLine({ model }: { model: HoverbarModel }) {
  const missing = model.freshness === "missing" || model.value === null;
  const meta = MODEL_LINE_META[model.id] ?? MODEL_LINE_META.model_usage_v4_flash;
  return (
    <div className="hb-model-line">
      <span className="hb-model-chip" data-variant={meta.variant} aria-hidden="true">
        {meta.variant === "vision" ? (
          <VisionApertureIcon size={26} />
        ) : meta.variant === "pro" ? (
          <ProCoreIcon size={26} />
        ) : (
          <FlashCrystalIcon size={26} />
        )}
      </span>
      <span className="hb-model-meta">
        <span className="hb-model-name">{meta.name}</span>
        {/* 宽停靠展示「语义 · 本月 Token」，侧边 300px 只保留语义（CSS 切换，不隐藏关键值） */}
        <span className="hb-model-sub">
          <span className="hb-model-sub-full">{meta.sub} · 本月 Token</span>
          <span className="hb-model-sub-short">{meta.sub}</span>
        </span>
      </span>
      <span
        className="hb-model-value"
        data-missing={missing || undefined}
        data-freshness={model.freshness}
        data-selectable="true"
      >
        {missing ? "暂不可用" : model.value}
      </span>
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
  // 判断先行：摘要页收敛为 当前判断 → 最近一次真实重置 → AI 状态；
  // landed_observed/user_confirmed 时主行已表达本机观察/确认，不重复显示同义的重置行。
  const decision = radar.decision;
  const badge = radarDecisionBadge(decision);
  const recentResetLine =
    decision.status === "landed_observed" || decision.status === "user_confirmed"
      ? null
      : decision.recentSummaryText;
  const recentGrantLine =
    decision.eventType === "banked_reset" &&
    (decision.status === "landed_observed" || decision.status === "user_confirmed")
      ? null
      : decision.recentBankedGrant
        ? radarBankedGrantLine(decision.recentBankedGrant, formatHoverbarClock)
        : null;
  const staleLine = radar.sourceStatus === "stale" ? radarSourceLine(radar) : null;
  return (
    <footer className="hb-radar-strip">
      <div className="hb-radar-strip-head">
        <Radar size={14} aria-hidden />
        <span className="hb-radar-strip-title">重置雷达</span>
        <span className="radar-phase-badge" data-phase={decision.status}>
          {badge}
        </span>
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
      <p className="hb-radar-strip-row-text hb-radar-strip-primary" data-selectable="true">
        <span className="hb-radar-strip-full">
          {radarRefreshing ? "正在同步 CodexRadar…" : radarDecisionStripLine(decision)}
        </span>
        <span className="hb-radar-strip-compact">
          {radarRefreshing ? "正在同步…" : radarDecisionStripLineCompact(decision)}
        </span>
      </p>
      {radarRefreshError ? (
        <p className="hb-radar-strip-error">{radarRefreshError}</p>
      ) : (
        <>
          {recentResetLine ? (
            <p className="hb-radar-strip-row-text" data-selectable="true">
              {recentResetLine}
            </p>
          ) : null}
          {recentGrantLine ? (
            <p className="hb-radar-strip-row-text" data-selectable="true">
              {recentGrantLine}
            </p>
          ) : null}
          <p className="hb-radar-strip-row-text" data-selectable="true">
            {radarAiStripLine(radar.aiAssessment)}
          </p>
        </>
      )}
      {staleLine ? <p className="hb-radar-strip-note">{staleLine}</p> : null}
    </footer>
  );
}

function planKey(plan: string): string {
  const key = plan.trim().toLowerCase();
  if (key.includes("pro")) return "pro";
  if (key.includes("plus")) return "plus";
  if (key.includes("ultra")) return "pro";
  if (key.includes("free")) return "free";
  if (key.includes("lite")) return "lite";
  return "other";
}

/**
 * 按后端账号列表构建分区：每个账号只聚合自己的 Source 与 Capability，
 * 账号顺序沿用后端（本机 → 默认 → 额外）。未配置来源的能力不展示，不补零。
 */
function buildSections(platform: PlatformSummaryViewModel): HoverbarSection[] {
  const multi = platform.accounts.length > 1;
  const configuredSourceIds = new Set(
    platform.sources.filter((source) => source.credentialConfigured).map((source) => source.sourceId),
  );
  const sections = platform.accounts.map((account) =>
    sectionFromAccount(account, platform.capabilities, configuredSourceIds),
  );
  if (!multi && sections.every((section) => isEmptySection(section))) {
    return [];
  }
  return sections;
}

function isEmptySection(section: HoverbarSection): boolean {
  return (
    section.windows.length === 0
    && section.bankedReset === null
    && section.credits === null
    && section.balance === null
    && section.spend.today === null
    && section.spend.month === null
    && section.models.length === 0
    && section.cacheHit === null
    && !section.plan
  );
}

function sectionFromAccount(
  account: PlatformSummaryViewModel["accounts"][number],
  capabilities: CapabilitySnapshotViewModel[],
  configuredSourceIds: Set<string>,
): HoverbarSection {
  const own = capabilities.filter(
    (item) =>
      item.accountId === account.accountId &&
      configuredSourceIds.has(item.sourceId) &&
      (ALLOWED_IDS.has(item.capabilityId) || isQuotaWindow(item.capabilityId)),
  );
  const windows = own
    .filter((item) => isQuotaWindow(item.capabilityId))
    .sort((left, right) => compareWindowIds(left.capabilityId, right.capabilityId))
    .map(toWindowMetric);
  // 今日/本月消费、模型行与缓存命中率目前只有 DeepSeek 官方用量来源产出；有真实现身才渲染对应结构
  const hasDeepseekExtras =
    own.some((item) => DEEPSEEK_EXTRA_IDS.has(item.capabilityId))
    || own.some((item) => DEEPSEEK_MODEL_IDS.has(item.capabilityId));
  const models = own
    .filter((item) => DEEPSEEK_MODEL_IDS.has(item.capabilityId))
    .sort(
      (left, right) =>
        DEEPSEEK_MODEL_ORDER.indexOf(left.capabilityId as (typeof DEEPSEEK_MODEL_ORDER)[number])
        - DEEPSEEK_MODEL_ORDER.indexOf(right.capabilityId as (typeof DEEPSEEK_MODEL_ORDER)[number]),
    )
    .map((item) => ({
      id: item.capabilityId,
      value: item.freshness === "missing" || !item.value.primary ? null : compactPercentText(item.value.primary),
      freshness: item.freshness,
    }));
  const staleCandidates = own.filter((item) => item.freshness === "stale");
  const staleAt = staleCandidates.reduce<number | null>((latest, item) => {
    const at = item.lastGoodAt ?? item.capturedAt;
    return at !== null && at !== undefined && (latest === null || at > latest) ? at : latest;
  }, null);
  const staleNote =
    staleCandidates.length > 0
      ? staleAt !== null
        ? `缓存 · 上次成功 ${formatHoverbarClock(staleAt)}`
        : "缓存 · 数据可能已过期"
      : null;
  return {
    id: account.accountId,
    title: accountTitle(account),
    plan: planOf(own),
    status: account.status,
    windows,
    bankedReset: bankedResetOf(own),
    credits: financeOf(own, "credits"),
    balance: financeOf(own, "balance"),
    spend: {
      today: hasDeepseekExtras ? financeOf(own, "today_spend") : null,
      month: hasDeepseekExtras ? financeOf(own, "month_spend") : null,
      total: financeOf(own, "total_spend"),
    },
    models,
    cacheHit: hasDeepseekExtras ? cacheOf(own, "cache_hit_rate") : null,
    staleNote,
  };
}

/** 窗口行：数值型剩余百分比 + 后端 secondary 中的重置时间；missing 保留行、灰轨道。 */
function toWindowMetric(capability: CapabilitySnapshotViewModel): HoverbarWindow {
  const missing = capability.freshness === "missing";
  const percent = capabilityRemainingPercent(capability);
  const raw = capability.value.primary;
  return {
    id: `${capability.sourceId}:${capability.capabilityId}`,
    label: windowMetricLabel(capability),
    percent: missing || percent === null ? null : percent,
    percentText: missing || !raw ? null : compactPercentText(raw),
    time: extractWindowTime(capability.value.secondary),
    freshness: capability.freshness,
  };
}

function bankedResetOf(capabilities: CapabilitySnapshotViewModel[]): HoverbarFinance | null {
  const capability = capabilities.find((item) => item.capabilityId === "banked_reset_count");
  if (!capability) return null;
  const raw = capability.value.primary?.trim() ?? "";
  const missing = capability.freshness === "missing" || raw === "";
  return {
    value: missing ? null : raw,
    freshness: capability.freshness,
  };
}

function financeOf(
  capabilities: CapabilitySnapshotViewModel[],
  id: string,
): HoverbarFinance | null {
  const capability = capabilities.find((item) => item.capabilityId === id);
  if (!capability) return null;
  const missing = capability.freshness === "missing" || !capability.value.primary;
  return {
    value: missing ? null : compactPercentText(capability.value.primary!),
    freshness: capability.freshness,
  };
}

function cacheOf(
  capabilities: CapabilitySnapshotViewModel[],
  id: string,
): HoverbarCache | null {
  const capability = capabilities.find((item) => item.capabilityId === id);
  if (!capability) return null;
  const missing = capability.freshness === "missing" || !capability.value.primary;
  const percent = capabilityRemainingPercent(capability);
  return {
    percent: missing || percent === null ? null : percent,
    percentText: missing ? null : compactPercentText(capability.value.primary!),
    desc: missing ? null : capability.value.secondary,
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
