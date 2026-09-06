/**
 * 通用能力仪表盘（CapabilityDashboard）。
 * 平台中心「额度与用量」按 Capability 类型组合渲染，不按 providerId 选择布局：
 * 新平台只要返回同类能力，前端就自动组合出同构页面。模块清单：
 * - WindowQuotaSection 窗口额度（quota_window_*）：额度卡 auto-fit，剩余/已使用/重置/采集 + 进度条；
 * - FinanceSection 资金账户（balance / today_spend / month_spend / total_spend）：连续资金面板，
 *   余额主值 + 消费次级行，金额右对齐 tabular（money / money-secondary，不套额度三段色）；
 * - CreditBalanceSection 额外额度（credits）：与订阅计划、资金余额分开，展示套餐外可用额度；
 * - ModelUsageSection 模型用量（model_usage_*）：紧凑模型行列表，V4 系列模型使用独立身份图标与语义副标题；
 * - EfficiencySection 调用与缓存效率（cache_hit_rate / *_cache_hit_rate / request_count / prompt_tokens /
 *   response_tokens / cache_hit_tokens / cache_miss_tokens）：仪表标题 + 命中率主值固定主蓝进度条（非额度语义），
 *   分模型命中率行统一靶心图标；
 * - TrendSection 趋势（usage_trend 或 value.kind === "trend"）：真实序列，无点显示空状态；
 * - 未知能力回退：Boxes 图标的紧凑行，不丢弃真实数据。
 * plan_level 不进入仪表盘：由账号头渲染为套餐徽章。missing 一律「未获取」不补零，
 * stale 保留真实值并标注最后成功时间。布局全部 auto-fit，随容器宽度响应，无平台专属断点。
 */
import type { ReactNode } from "react";
import {
  Activity,
  Boxes,
  CalendarClock,
  CalendarDays,
  CalendarRange,
  CircleDollarSign,
  Clock3,
  Coins,
  Cpu,
  Hourglass,
  Landmark,
  ReceiptText,
  TimerReset,
  WalletCards,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import {
  EfficiencyGaugeIcon,
  FlashCrystalIcon,
  ProCoreIcon,
  TargetRingIcon,
  VisionApertureIcon,
  modelIconKind,
  type ModelIconKind,
} from "@/components/ui/MetricIcons";
import { QuotaProgress, capabilityRemainingPercent, quotaTone, quotaToneColor } from "@/components/ui/QuotaProgress";
import { FreshnessTag } from "@/components/ui/StatusBadge";
import { compactPercentText, formatTime } from "@/lib/format";
import type { CapabilitySnapshotViewModel } from "@/lib/types";
import { UsageTrend } from "./UsageTrend";

/* ————————————————— 能力类型体系（capabilityId 语义归类，与平台名无关） ————————————————— */

type CapabilityGroup = "window" | "finance" | "credits" | "banked" | "model_usage" | "efficiency" | "trend" | "other";

const FINANCE_MAIN_ID = "balance";
const FINANCE_SECONDARY_META: Array<{ id: string; icon: LucideIcon }> = [
  { id: "today_spend", icon: ReceiptText },
  { id: "month_spend", icon: CalendarClock },
  { id: "total_spend", icon: Landmark },
];
/** 紧凑统计矩阵（能力归类用；渲染上由分模型明细行承载，不再单独出矩阵）。 */
const EFFICIENCY_STAT_META: Array<{ id: string; icon: LucideIcon }> = [
  { id: "request_count", icon: Activity },
  { id: "prompt_tokens", icon: Activity },
  { id: "response_tokens", icon: Activity },
  { id: "cache_hit_tokens", icon: Activity },
  { id: "cache_miss_tokens", icon: Activity },
];

/**
 * V4 系列模型身份（重设计 V2）：独立身份图标 + 语义副标题 + 芯片底色。
 * Flash 蓝青晶体翼 / Vision 青色光圈 / Pro 紫色神经旋涡，禁止互相复用；
 * 其它 model_usage_* 能力回退线性 Cpu 行，不影响未来平台。
 */
type ModelIdentity = { name: string; sub: string; chip: string; kind: ModelIconKind };

const MODEL_IDENTITY: Record<string, ModelIdentity> = {
  model_usage_v4_flash: { name: "V4 Flash", sub: "旗舰轻量模型", chip: "rgba(10, 102, 255, 0.1)", kind: "flash" },
  model_usage_v4_flash_vision: {
    name: "V4 Flash Vision",
    sub: "视觉模型",
    chip: "rgba(8, 145, 178, 0.12)",
    kind: "vision",
  },
  model_usage_v4_pro: { name: "V4 Pro", sub: "深度思考模型", chip: "rgba(124, 58, 237, 0.12)", kind: "pro" },
};

function ModelIdentityGlyph({ id }: { id: string }) {
  const kind = modelIconKind(id);
  if (kind === "vision") return <VisionApertureIcon size={24} />;
  if (kind === "pro") return <ProCoreIcon size={24} />;
  if (kind === "flash") return <FlashCrystalIcon size={24} />;
  return <Cpu size={15} />;
}

function classifyCapability(capability: CapabilitySnapshotViewModel): CapabilityGroup {
  const id = capability.capabilityId;
  if (id === "plan_level") return "other";
  if (id.startsWith("quota_window_")) return "window";
  if (id === "cache_hit_rate" || id.endsWith("_cache_hit_rate")) return "efficiency";
  // 请求数与四类 Token 计数归入调用效率模块的统计矩阵（模块渲染规则）
  if (EFFICIENCY_STAT_META.some((meta) => meta.id === id)) return "efficiency";
  if (id.startsWith("model_usage_")) return "model_usage";
  if (id === FINANCE_MAIN_ID || FINANCE_SECONDARY_META.some((meta) => meta.id === id)) return "finance";
  if (id === "credits") return "credits";
  if (id === "banked_reset_count") return "banked";
  if (id === "usage_trend" || capability.value.kind === "trend") return "trend";
  return "other";
}

function groupCapabilities(capabilities: CapabilitySnapshotViewModel[]): Record<CapabilityGroup, CapabilitySnapshotViewModel[]> {
  const groups: Record<CapabilityGroup, CapabilitySnapshotViewModel[]> = {
    window: [],
    finance: [],
    credits: [],
    banked: [],
    model_usage: [],
    efficiency: [],
    trend: [],
    other: [],
  };
  for (const capability of capabilities) {
    groups[classifyCapability(capability)].push(capability);
  }
  return groups;
}

/* ————————————————— 通用小件 ————————————————— */

function findCapability(capabilities: CapabilitySnapshotViewModel[], id: string): CapabilitySnapshotViewModel | null {
  return capabilities.find((capability) => capability.capabilityId === id) ?? null;
}

/** 没有真实值（能力缺失 / missing / 主值为空）时不补零。 */
function isMissing(capability: CapabilitySnapshotViewModel | null): boolean {
  return capability === null || capability.freshness === "missing" || capability.value.primary === null;
}

function primaryText(capability: CapabilitySnapshotViewModel): string {
  return compactPercentText(capability.value.primary ?? "");
}

/** 资金/效率项时间线：stale 标注缓存与最后成功时间，fresh 展示采集时间，missing 不展示。 */
function freshnessLine(capability: CapabilitySnapshotViewModel | null): { text: string; stale: boolean } | null {
  if (capability === null || capability.freshness === "missing" || capability.value.primary === null) {
    return null;
  }
  if (capability.freshness === "stale") {
    const at = capability.lastGoodAt ?? capability.capturedAt;
    return { text: `缓存 · 上次成功 ${formatTime(at)}`, stale: true };
  }
  return capability.capturedAt !== null ? { text: formatTime(capability.capturedAt), stale: false } : null;
}

/** 模块容器：图标软底座 + 标题 + 右侧徽章插槽，两主题由 Token 驱动。
 *  图标接受 Lucide 或本模块 SVG 组件（签名宽于 LucideIcon 的 size 联合类型）。 */
function ModulePanel({
  icon: Icon,
  title,
  aside,
  children,
  className,
}: {
  icon: (props: { size?: number; className?: string }) => ReactNode;
  title: string;
  aside?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section className={`glass-panel flex min-w-0 flex-col gap-3 p-4 ${className ?? ""}`}>
      <div className="flex items-center gap-2.5">
        <span
          aria-hidden
          className="flex h-7 w-7 shrink-0 items-center justify-center rounded-[9px] border border-q-border/70 bg-q-surface-muted text-q-text-secondary"
        >
          <Icon size={15} />
        </span>
        <h3 className="min-w-0 text-[14px] font-semibold tracking-tight text-q-text-primary">{title}</h3>
        {aside}
      </div>
      {children}
    </section>
  );
}

/** 窗口类型图标：5 小时 → 7 天 → 30 天 → 其他动态窗口。 */
function windowIcon(capabilityId: string): LucideIcon {
  if (capabilityId === "quota_window_5h") return Clock3;
  if (capabilityId === "quota_window_7d") return CalendarDays;
  if (capabilityId === "quota_window_30d") return CalendarRange;
  return Hourglass;
}

/* ————————————————— 窗口额度模块 ————————————————— */

function WindowQuotaCard({ capability }: { capability: CapabilitySnapshotViewModel }) {
  const Icon = windowIcon(capability.capabilityId);
  const missing = isMissing(capability);
  const remaining = missing ? null : capabilityRemainingPercent(capability);
  const tone = quotaTone(remaining);
  const color = quotaToneColor(tone);
  const line = freshnessLine(capability);
  return (
    <div className="flex min-w-0 flex-col gap-2 rounded-[12px] border border-q-border bg-q-surface-muted/60 p-3.5">
      <div className="flex min-w-0 items-center gap-2">
        <Icon size={14} aria-hidden className="shrink-0 text-q-text-muted" />
        <span className="min-w-0 flex-1 truncate text-[13px] font-medium text-q-text-secondary">
          {capability.displayName}
        </span>
        <FreshnessTag freshness={capability.freshness} />
      </div>
      <p
        className="text-[26px] font-bold leading-8 tracking-tight tabular-nums"
        style={missing ? { color: "var(--q-text-muted)", fontWeight: 400 } : { color: color ?? "var(--q-text-primary)" }}
        data-selectable="true"
      >
        {missing ? "未获取" : primaryText(capability)}
      </p>
      <QuotaProgress capability={capability} />
      {capability.value.secondary && !missing && (
        <p className="truncate text-[11px] leading-4 text-q-text-muted" title={capability.value.secondary}>
          {capability.value.secondary}
        </p>
      )}
      {line && (
        <p className="text-[11px] leading-4" style={{ color: line.stale ? "var(--q-warning)" : "var(--q-text-muted)" }}>
          {line.text}
        </p>
      )}
    </div>
  );
}

function WindowQuotaSection({ capabilities }: { capabilities: CapabilitySnapshotViewModel[] }) {
  return (
    <ModulePanel icon={TimerReset} title="窗口额度">
      <div className="grid grid-cols-[repeat(auto-fit,minmax(min(100%,250px),1fr))] gap-4">
        {capabilities.map((capability) => (
          <WindowQuotaCard key={`${capability.sourceId}-${capability.capabilityId}`} capability={capability} />
        ))}
      </div>
    </ModulePanel>
  );
}

/* ————————————————— 资金账户模块 ————————————————— */

function FinanceSection({ capabilities }: { capabilities: CapabilitySnapshotViewModel[] }) {
  const balance = findCapability(capabilities, FINANCE_MAIN_ID);
  const secondary = FINANCE_SECONDARY_META.map(({ id, icon }) => {
    const capability = findCapability(capabilities, id);
    return capability ? { capability, icon } : null;
  }).filter((item): item is { capability: CapabilitySnapshotViewModel; icon: LucideIcon } => item !== null);

  // 若没有 balance 字段，但有消费类能力（如纯 total_spend），提升首个消费字段为主值
  const primaryCap = balance ?? secondary[0]?.capability ?? null;
  const otherSecondary = balance ? secondary : secondary.slice(1);
  const primaryMissing = isMissing(primaryCap);
  const primaryLine = freshnessLine(primaryCap);
  const asideCap = primaryCap;

  return (
    <ModulePanel
      icon={WalletCards}
      title="资金账户"
      aside={asideCap && asideCap.freshness !== "fresh" ? <FreshnessTag freshness={asideCap.freshness} /> : null}
    >
      {/* 主值：冰霜白蓝 money Token，右对齐 tabular，字重约 650，无渐变无发光 */}
      {primaryCap && (
        <div className="flex min-w-0 flex-col gap-1 border-t border-q-border pt-3">
          <div className="flex min-w-0 items-center gap-2">
            <CircleDollarSign size={14} aria-hidden className="shrink-0 text-q-text-muted" />
            <span className="min-w-0 flex-1 truncate text-xs text-q-text-muted">
              {primaryCap.displayName || (balance ? "余额" : "消费")}
            </span>
          </div>
          <div className="flex min-w-0 items-baseline justify-between gap-3">
            <span
              className="min-w-0 truncate text-[28px] leading-9 tracking-tight text-[var(--q-money)]"
              style={{ fontWeight: 650, fontVariantNumeric: "tabular-nums" }}
              data-selectable="true"
              data-missing={primaryMissing || undefined}
            >
              {primaryMissing ? "未获取" : primaryText(primaryCap)}
            </span>
          </div>
          {primaryLine && (
            <p
              className="text-[11px]"
              style={{ color: primaryLine.stale ? "var(--q-warning)" : "var(--q-text-muted)" }}
            >
              {primaryLine.text}
            </p>
          )}
        </div>
      )}

      {/* 消费次级行：只渲染真实存在的字段，次级 money-secondary */}
      {otherSecondary.length > 0 && (
        <div className="flex min-w-0 flex-col border-t border-q-border pt-2">
          {otherSecondary.map(({ capability, icon: Icon }, index) => {
            const missing = isMissing(capability);
            const line = freshnessLine(capability);
            return (
              <div
                key={capability.capabilityId}
                className={`flex min-w-0 flex-col gap-0.5 py-2 ${index > 0 ? "border-t border-q-border/60" : ""}`}
              >
                <div className="flex min-w-0 items-baseline justify-between gap-3">
                  <span className="flex min-w-0 items-center gap-2">
                    <Icon size={13} aria-hidden className="shrink-0 text-q-text-muted" />
                    <span className="truncate text-xs text-q-text-muted">{capability.displayName}</span>
                    {capability.freshness !== "fresh" && <FreshnessTag freshness={capability.freshness} />}
                  </span>
                  <span
                    className="min-w-0 shrink-0 truncate text-right text-[16px] leading-6 text-[var(--q-money-secondary)]"
                    style={{ fontWeight: 650, fontVariantNumeric: "tabular-nums" }}
                    data-selectable="true"
                    data-missing={missing || undefined}
                  >
                    {missing ? "未获取" : primaryText(capability)}
                  </span>
                </div>
                {line && (
                  <p
                    className="text-right text-[10.5px]"
                    style={{ color: line.stale ? "var(--q-warning)" : "var(--q-text-muted)" }}
                  >
                    {line.text}
                  </p>
                )}
              </div>
            );
          })}
        </div>
      )}
    </ModulePanel>
  );
}

/* ————————————————— 可用重置卡 ————————————————— */

function BankedResetSection({ capability }: { capability: CapabilitySnapshotViewModel }) {
  const missing = isMissing(capability);
  const line = freshnessLine(capability);
  const countText = missing ? "未获取" : `${primaryText(capability)} 张`;
  return (
    <ModulePanel icon={TimerReset} title="可用重置卡">
      <div className="flex min-w-0 flex-wrap items-baseline justify-between gap-x-3 gap-y-1 border-t border-q-border pt-3">
        <span className="flex shrink-0 items-baseline gap-2">
          <span
            className="text-[18px] leading-6 text-q-text-primary"
            style={{ fontWeight: 650, fontVariantNumeric: "tabular-nums" }}
            data-selectable="true"
            data-missing={missing || undefined}
          >
            {countText}
          </span>
          {capability.freshness !== "fresh" && <FreshnessTag freshness={capability.freshness} />}
        </span>
        <p className="min-w-0 truncate text-[12.5px] text-q-text-secondary" title={capability.value.secondary ?? undefined}>
          {missing ? "暂无法获取" : capability.value.secondary ?? "可用于重置 Codex 使用额度"}
          {line ? <span className="text-[11px] text-q-text-muted"> · {line.text}</span> : null}
        </p>
      </div>
    </ModulePanel>
  );
}

/* ————————————————— 额外额度模块（Credits） ————————————————— */

function CreditBalanceSection({ capability }: { capability: CapabilitySnapshotViewModel }) {
  const missing = isMissing(capability);
  const line = freshnessLine(capability);
  const note = line
    ? `套餐内额度用尽后用于继续使用 Codex · ${line.text}`
    : "套餐内额度用尽后用于继续使用 Codex";
  return (
    <ModulePanel icon={Coins} title="额外额度">
      <div className="flex min-w-0 flex-wrap items-baseline justify-between gap-x-3 gap-y-1 border-t border-q-border pt-3">
        <span className="flex shrink-0 items-baseline gap-2">
          <span
            className="text-[18px] leading-6 text-q-text-primary"
            style={{ fontWeight: 650, fontVariantNumeric: "tabular-nums" }}
            data-selectable="true"
            data-missing={missing || undefined}
          >
            {missing ? "未获取" : primaryText(capability)}
          </span>
          {capability.freshness !== "fresh" && <FreshnessTag freshness={capability.freshness} />}
        </span>
        <p className="min-w-0 truncate text-[12.5px] text-q-text-secondary" title={note}>
          套餐内额度用尽后用于继续使用 Codex
          {line ? (
            <span className="text-[11px] text-q-text-muted"> · {line.text}</span>
          ) : null}
        </p>
      </div>
    </ModulePanel>
  );
}

/* ————————————————— 模型用量模块 ————————————————— */

function ModelUsageSection({ capabilities }: { capabilities: CapabilitySnapshotViewModel[] }) {
  return (
    <ModulePanel icon={Cpu} title="模型用量">
      {/* flex-1：双列等高时行均分剩余高度（单列时行高由 min-h 决定），面板外不留空白 */}
      <div className="flex min-w-0 flex-1 flex-col gap-2 border-t border-q-border pt-3">
        {capabilities.map((capability) => {
          const missing = isMissing(capability);
          const line = freshnessLine(capability);
          const identity = MODEL_IDENTITY[capability.capabilityId] ?? null;
          // V4 系列模型：身份图标方块 + 语义副标题；其它模型回退线性行，不丢弃数据
          if (!identity) {
            return (
              <div
                key={`${capability.sourceId}-${capability.capabilityId}`}
                className="flex min-h-[64px] min-w-0 flex-1 flex-col justify-center gap-0.5 rounded-[12px] bg-q-surface-muted px-3 py-2.5 shadow-[inset_0_0_0_1px_var(--q-border)]"
              >
                <div className="flex min-w-0 items-baseline justify-between gap-3">
                  <span className="flex min-w-0 items-center gap-2">
                    <Cpu size={13} aria-hidden className="shrink-0 text-q-text-muted" />
                    <span className="truncate text-[13px] font-medium text-q-text-primary">{capability.displayName}</span>
                    {capability.freshness !== "fresh" && <FreshnessTag freshness={capability.freshness} />}
                  </span>
                  <span
                    className="min-w-0 shrink-0 truncate text-right text-[16px] leading-6 text-q-text-primary"
                    style={{ fontWeight: 650, fontVariantNumeric: "tabular-nums" }}
                    data-selectable="true"
                    data-missing={missing || undefined}
                  >
                    {missing ? "未获取" : primaryText(capability)}
                  </span>
                </div>
                {capability.value.secondary && (
                  <p className="truncate text-[11px] text-q-text-muted" title={capability.value.secondary}>
                    {capability.value.secondary}
                  </p>
                )}
                {line && (
                  <p className="text-[10.5px]" style={{ color: line.stale ? "var(--q-warning)" : "var(--q-text-muted)" }}>
                    {line.text}
                  </p>
                )}
              </div>
            );
          }
          return (
            <div
              key={`${capability.sourceId}-${capability.capabilityId}`}
              className="flex min-h-[64px] min-w-0 flex-1 flex-col justify-center gap-1 rounded-[12px] bg-q-surface-muted px-3 py-2.5 shadow-[inset_0_0_0_1px_var(--q-border)]"
            >
              <div className="flex min-w-0 items-center gap-3">
                <span
                  aria-hidden
                  className="flex h-10 w-10 shrink-0 items-center justify-center rounded-[10px]"
                  style={{ background: identity.chip }}
                >
                  <ModelIdentityGlyph id={capability.capabilityId} />
                </span>
                <span className="flex min-w-0 flex-1 flex-col">
                  <span className="flex min-w-0 items-center gap-2">
                    <span className="truncate text-[14px] font-semibold text-q-text-primary">{identity.name}</span>
                    {capability.freshness !== "fresh" && <FreshnessTag freshness={capability.freshness} />}
                  </span>
                  <span className="truncate text-[11px] text-q-text-muted">{identity.sub} · 本月 Token</span>
                </span>
                <span
                  className="min-w-0 shrink-0 truncate text-right text-[19px] leading-6 text-q-text-primary"
                  style={{ fontWeight: 700, fontVariantNumeric: "tabular-nums" }}
                  data-selectable="true"
                  data-missing={missing || undefined}
                >
                  {missing ? "未获取" : primaryText(capability)}
                </span>
              </div>
              {line && (
                <p className="text-[10.5px]" style={{ color: line.stale ? "var(--q-warning)" : "var(--q-text-muted)" }}>
                  {line.text}
                </p>
              )}
            </div>
          );
        })}
      </div>
    </ModulePanel>
  );
}

/* ————————————————— 调用效率模块 ————————————————— */

/** 固定主蓝细进度条：缓存命中率是非额度语义，不套剩余额度三段色阶。 */
function RateBar({ percent, label }: { percent: number | null; label: string }) {
  return (
    <div
      className="h-1.5 min-w-0 overflow-hidden rounded-full bg-[var(--q-quota-track)]"
      role="progressbar"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={percent === null ? undefined : Math.round(percent)}
      aria-label={label}
    >
      <div
        className="h-full rounded-full transition-[width] duration-300"
        style={{ width: `${percent === null ? 0 : Math.min(100, Math.max(0, percent))}%`, background: "var(--q-primary)" }}
      />
    </div>
  );
}

function EfficiencySection({ capabilities }: { capabilities: CapabilitySnapshotViewModel[] }) {
  const rate = findCapability(capabilities, "cache_hit_rate");
  const rateMissing = isMissing(rate);
  const rateLine = freshnessLine(rate);
  const modelRates = capabilities.filter(
    (capability) => capability.capabilityId !== "cache_hit_rate" && capability.capabilityId.endsWith("_cache_hit_rate"),
  );
  const staleAt = capabilities.reduce<number | null>((latest, capability) => {
    if (capability.freshness !== "stale") return latest;
    const at = capability.lastGoodAt ?? capability.capturedAt;
    return at !== null && at !== undefined && (latest === null || at > latest) ? at : latest;
  }, null);
  return (
    <ModulePanel icon={EfficiencyGaugeIcon} title="调用与缓存效率">
      {/* 全局命中率主值：较大靶心 + 全部模型口径 */}
      <div className="flex min-w-0 flex-col gap-2 border-t border-q-border pt-3">
        <div className="flex min-w-0 items-center justify-between gap-3">
          <span className="flex min-w-0 items-center gap-2">
            <TargetRingIcon size={20} />
            <span className="truncate text-xs text-q-text-muted">{rate?.displayName ?? "缓存命中率"}（全部模型）</span>
          </span>
          <span
            className="min-w-0 shrink-0 text-[22px] leading-7 tracking-tight text-q-text-primary"
            style={{ fontWeight: 650, fontVariantNumeric: "tabular-nums" }}
            data-selectable="true"
            data-missing={rateMissing || undefined}
          >
            {rateMissing ? "未获取" : rate ? primaryText(rate) : ""}
          </span>
        </div>
        <RateBar
          percent={rate && !rateMissing && rate.value.progress !== null ? rate.value.progress * 100 : null}
          label="缓存命中率"
        />
        {rate?.value.secondary && !rateMissing && (
          <p className="truncate text-[11px] text-q-text-muted" title={rate.value.secondary}>
            {rate.value.secondary}
          </p>
        )}
        {rateLine?.stale && <p className="text-[11px] text-q-warning">{rateLine.text}</p>}
      </div>

      {/* 分模型命中率行：图标统一小靶心，模型名靠文字区分；未调用不画空条 */}
      {modelRates.length > 0 && (
        <div className="flex min-w-0 flex-1 flex-col gap-2 border-t border-q-border pt-3">
          {modelRates.map((capability) => {
            const missing = capability.freshness === "missing";
            const primary = capability.value.primary;
            const secondary = capability.value.secondary;
            // 未调用：后端 primary/secondary 均为空；不补零、不突出 0%
            const uncalled = !missing && primary === null && secondary === null;
            return (
              <div
                key={`${capability.sourceId}-${capability.capabilityId}`}
                className="flex min-h-[64px] min-w-0 flex-1 flex-col justify-center gap-1 rounded-[12px] bg-q-surface-muted px-3 py-2.5 shadow-[inset_0_0_0_1px_var(--q-border)]"
              >
                <div className="flex min-w-0 items-center gap-2.5">
                  <TargetRingIcon size={15} className="shrink-0" />
                  <span className="min-w-0 flex-1 truncate text-[12.5px] font-medium text-q-text-primary">
                    {capability.displayName}
                  </span>
                  <span
                    className="shrink-0 text-[14px] tabular-nums text-q-text-primary"
                    style={{ fontWeight: 650 }}
                    data-selectable="true"
                    data-missing={(missing || primary === null) || undefined}
                  >
                    {missing ? "未获取" : uncalled ? "未调用" : primaryText(capability)}
                  </span>
                </div>
                {secondary && (
                  <p className="truncate pl-[25px] text-[10.5px] text-q-text-muted" title={secondary} data-selectable="true">
                    {secondary}
                  </p>
                )}
              </div>
            );
          })}
        </div>
      )}

      {staleAt ? <p className="text-[11px] text-q-warning">缓存 · 上次成功 {formatTime(staleAt)}</p> : null}
    </ModulePanel>
  );
}

/* ————————————————— 未知能力回退 ————————————————— */

function OtherCapabilitySection({ capabilities }: { capabilities: CapabilitySnapshotViewModel[] }) {
  return (
    <ModulePanel icon={Boxes} title="其他数据">
      <div className="flex min-w-0 flex-col gap-2 border-t border-q-border pt-3">
        {capabilities.map((capability) => {
          const missing = isMissing(capability);
          return (
            <div key={`${capability.sourceId}-${capability.capabilityId}`} className="flex min-w-0 items-baseline justify-between gap-3">
              <span className="flex min-w-0 items-center gap-2">
                <Boxes size={13} aria-hidden className="shrink-0 text-q-text-muted" />
                <span className="truncate text-xs text-q-text-muted">{capability.displayName}</span>
                {capability.freshness !== "fresh" && <FreshnessTag freshness={capability.freshness} />}
              </span>
              <span
                className="min-w-0 shrink-0 truncate text-right text-[13px] tabular-nums text-q-text-primary"
                data-selectable="true"
                data-missing={missing || undefined}
              >
                {missing ? "未获取" : primaryText(capability)}
              </span>
            </div>
          );
        })}
      </div>
    </ModulePanel>
  );
}

/* ————————————————— 组合入口 ————————————————— */

/**
 * 账号能力组合渲染：窗口额度 → 资金账户 → 额外额度 → 模型用量 → 调用效率 → 趋势 → 其他。
 * 只渲染账号真实拥有的模块；没有的能力不渲染、不补空卡。
 * 布局与容器宽度联动（wide 由 UsageView 的 useContainerWidth 驱动）：
 * - wide：资金全宽一行 → 模型用量 | 调用与缓存效率 双列等高互撑（行均分高度，不留面板外空白）；
 * - 非 wide：全部单列，行高由内容决定。
 */
export function CapabilityDashboard({
  capabilities,
  wide = false,
}: {
  capabilities: CapabilitySnapshotViewModel[];
  wide?: boolean;
}) {
  const groups = groupCapabilities(capabilities);
  const credits = groups.credits[0] ?? null;
  const banked = groups.banked[0] ?? null;
  const trend = groups.trend[0] ?? null;
  if (capabilities.length === 0) {
    return <p className="px-1 text-xs text-q-text-muted">该账号暂无额度数据。</p>;
  }
  const hasUsagePanels = groups.model_usage.length > 0 || groups.efficiency.length > 0;
  return (
    <div className="flex min-w-0 flex-col gap-4">
      {groups.window.length > 0 && <WindowQuotaSection capabilities={groups.window} />}

      {(groups.finance.length > 0 || hasUsagePanels) && (
        <div className="flex min-w-0 flex-col gap-4">
          {groups.finance.length > 0 && <FinanceSection capabilities={groups.finance} />}
          {hasUsagePanels && (
            wide ? (
              <div className="grid grid-cols-2 items-stretch gap-4">
                {groups.model_usage.length > 0 && <ModelUsageSection capabilities={groups.model_usage} />}
                {groups.efficiency.length > 0 && <EfficiencySection capabilities={groups.efficiency} />}
              </div>
            ) : (
              <div className="flex min-w-0 flex-col gap-4">
                {groups.model_usage.length > 0 && <ModelUsageSection capabilities={groups.model_usage} />}
                {groups.efficiency.length > 0 && <EfficiencySection capabilities={groups.efficiency} />}
              </div>
            )
          )}
        </div>
      )}

      {banked && <BankedResetSection capability={banked} />}
      {credits && <CreditBalanceSection capability={credits} />}
      {trend && <UsageTrend capability={trend} />}
      {groups.other.length > 0 && <OtherCapabilitySection capabilities={groups.other} />}
    </div>
  );
}
