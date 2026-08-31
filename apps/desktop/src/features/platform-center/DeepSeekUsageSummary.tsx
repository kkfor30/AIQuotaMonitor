/**
 * DeepSeek 用量组合渲染（2026-09-01 用量展示重设计 V1）。
 * 在统一 UsageView 内按 capability 组合渲染，不复制整张平台专属页面（D006）：
 * 1. 资金概览：balance 主值 + 今日/本月/累计消费次级值，每项保留自己的 freshness/capturedAt；
 * 2. 模型用量：V4 Flash / V4 Pro 两条横向模型行，只展示真实 Token 文本；
 * 3. 调用与缓存效率：cache_hit_rate 为主值（固定主蓝进度条，不套额度三段色阶），
 *    紧凑统计请求数/输入/输出 Token，hit/miss Token 真实存在时才展示；
 * 4. 近 7 日消费趋势：沿用 UsageTrend，无点时显示空状态，不插值、不补零。
 * missing 一律显示「未获取」，stale 保留真实值并标注缓存与最后成功时间。
 */
import { BarChart3, Wallet } from "lucide-react";
import { FlashCrystalIcon, ProCoreIcon, TargetRingIcon } from "@/components/ui/MetricIcons";
import { FreshnessTag } from "@/components/ui/StatusBadge";
import { compactPercentText, formatTime } from "@/lib/format";
import type { CapabilitySnapshotViewModel } from "@/lib/types";
import { UsageTrend } from "./UsageTrend";

/** 参与资金概览组合的能力；total_spend 为可选第四项。 */
const FINANCE_SECONDARY_IDS = ["today_spend", "month_spend", "total_spend"] as const;
const MODEL_IDS = ["model_usage_v4_flash", "model_usage_v4_flash_vision", "model_usage_v4_pro"] as const;
const STAT_IDS = ["request_count", "prompt_tokens", "response_tokens"] as const;
const CACHE_TOKEN_IDS = ["cache_hit_tokens", "cache_miss_tokens"] as const;

/** 模型行槽位元数据：名称 + 图标芯片底色（Vision 为 Flash 衍生，青绿底区分）。 */
const MODEL_META: Record<string, { name: string; chip: string }> = {
  model_usage_v4_flash: { name: "V4 Flash", chip: "rgba(10, 102, 255, 0.1)" },
  model_usage_v4_flash_vision: { name: "V4 Flash Vision", chip: "rgba(13, 148, 136, 0.12)" },
  model_usage_v4_pro: { name: "V4 Pro", chip: "rgba(124, 58, 237, 0.12)" },
};

function findCapability(
  capabilities: CapabilitySnapshotViewModel[],
  id: string,
): CapabilitySnapshotViewModel | null {
  return capabilities.find((capability) => capability.capabilityId === id) ?? null;
}

/** 没有真实值（能力缺失 / missing / 主值为空）时不补零。 */
function isMissing(capability: CapabilitySnapshotViewModel | null): boolean {
  return capability === null || capability.freshness === "missing" || capability.value.primary === null;
}

function primaryText(capability: CapabilitySnapshotViewModel): string {
  return compactPercentText(capability.value.primary ?? "");
}

/** 每个资金项的时间线：stale 标注缓存与最后成功时间，fresh 展示采集时间，missing 不展示。 */
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

/** 资金项：标签 + 数值 + 新鲜度时间线；main 为余额主值。 */
function MoneyItem({
  capability,
  main = false,
}: {
  capability: CapabilitySnapshotViewModel | null;
  main?: boolean;
}) {
  const missing = isMissing(capability);
  const line = freshnessLine(capability);
  return (
    <div className="flex min-w-0 flex-col gap-1">
      <div className="flex items-center gap-2">
        <span className="truncate text-xs text-q-text-muted">{capability?.displayName ?? "余额"}</span>
        {capability && capability.freshness !== "fresh" && <FreshnessTag freshness={capability.freshness} />}
      </div>
      <p
        className={
          main
            ? "text-[30px] font-bold leading-9 tracking-tight tabular-nums text-[var(--q-money)]"
            : "text-[20px] font-bold leading-7 tracking-tight tabular-nums text-q-text-primary"
        }
        data-selectable="true"
        data-missing={missing || undefined}
      >
        {missing ? "未获取" : primaryText(capability!)}
      </p>
      {line && (
        <p className={line.stale ? "text-[11px] text-q-warning" : "text-[11px] text-q-text-muted"}>{line.text}</p>
      )}
    </div>
  );
}

/** 模型行：指标图标芯片 + 名称/语义 + 右对齐真实 Token 文本；名称按能力槽位固定。 */
function ModelRow({ capability, id }: { capability: CapabilitySnapshotViewModel | null; id: string }) {
  const missing = isMissing(capability);
  const meta = MODEL_META[id] ?? MODEL_META.model_usage_v4_flash;
  return (
    <div className="flex min-w-0 items-center gap-3 rounded-[12px] bg-q-surface-muted px-3 py-2.5 shadow-[inset_0_0_0_1px_var(--q-border)]">
      <span
        aria-hidden="true"
        className="flex h-8 w-8 shrink-0 items-center justify-center rounded-[9px]"
        style={{ background: meta.chip }}
      >
        {id === "model_usage_v4_pro" ? <ProCoreIcon size={17} /> : <FlashCrystalIcon size={17} />}
      </span>
      <div className="flex min-w-0 flex-1 flex-col">
        <span className="truncate text-[13px] font-semibold text-q-text-primary">{meta.name}</span>
        <span className="truncate text-[11px] text-q-text-muted">本月累计 Token</span>
      </div>
      <div className="flex shrink-0 flex-col items-end gap-0.5">
        <span
          className="text-[19px] font-bold leading-6 tracking-tight tabular-nums text-q-text-primary"
          data-selectable="true"
          data-missing={missing || undefined}
        >
          {missing ? "未获取" : primaryText(capability!)}
        </span>
        {capability && capability.freshness !== "fresh" && <FreshnessTag freshness={capability.freshness} />}
      </div>
    </div>
  );
}

/** 紧凑统计单元：真实存在才计入网格；missing 显示「未获取」。 */
function StatCell({ label, value, missing }: { label: string; value: string | null; missing: boolean }) {
  return (
    <div className="flex min-w-0 flex-col gap-0.5">
      <span className="truncate text-[11px] text-q-text-muted">{label}</span>
      <span
        className="truncate text-[15px] font-bold leading-5 tabular-nums text-q-text-primary"
        data-selectable="true"
        data-missing={missing || undefined}
      >
        {missing ? "未获取" : value}
      </span>
    </div>
  );
}

/** 调用与缓存效率面板：cache_hit_rate 主值 + 紧凑统计 + 可选 hit/miss Token。 */
function CacheEfficiencyPanel({ capabilities }: { capabilities: CapabilitySnapshotViewModel[] }) {
  const rate = findCapability(capabilities, "cache_hit_rate");
  const rateMissing = isMissing(rate);
  const rateLine = freshnessLine(rate);
  const stats = STAT_IDS.map((id) => findCapability(capabilities, id));
  const cacheTokens = CACHE_TOKEN_IDS.map((id) => findCapability(capabilities, id)).filter(
    (capability): capability is CapabilitySnapshotViewModel =>
      capability !== null && capability.freshness !== "missing" && capability.value.primary !== null,
  );
  const staleAt = capabilities.reduce<number | null>((latest, capability) => {
    if (capability.freshness !== "stale") return latest;
    const at = capability.lastGoodAt ?? capability.capturedAt;
    return at !== null && at !== undefined && (latest === null || at > latest) ? at : latest;
  }, null);
  return (
    <section className="glass-panel flex min-w-0 flex-col gap-3 p-4">
      <div className="flex items-center gap-2">
        <TargetRingIcon size={15} />
        <h3 className="text-[14px] font-semibold tracking-tight text-q-text-primary">调用与缓存效率</h3>
      </div>

      <div className="flex flex-col gap-2">
        <div className="flex items-center justify-between gap-3">
          <span className="text-xs text-q-text-muted">{rate?.displayName ?? "缓存命中率"}</span>
          <span
            className="text-[24px] font-bold leading-8 tracking-tight tabular-nums text-q-text-primary"
            data-selectable="true"
            data-missing={rateMissing || undefined}
          >
            {rateMissing ? "未获取" : primaryText(rate!)}
          </span>
        </div>
        {/* 非额度语义：固定主蓝填充，不套 QuotaProgress 三段色阶 */}
        <div
          className="h-1.5 overflow-hidden rounded-full bg-[var(--q-quota-track)]"
          role="progressbar"
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={rateMissing ? undefined : Math.round(rate?.value.progress != null ? rate.value.progress * 100 : 0)}
          aria-label="缓存命中率"
        >
          <div
            className="h-full rounded-full transition-[width] duration-300"
            style={{
              width: `${rateMissing ? 0 : Math.min(100, Math.max(0, (rate?.value.progress ?? 0) * 100))}%`,
              background: "var(--q-primary)",
            }}
          />
        </div>
        {rate?.value.secondary && !rateMissing && (
          <p className="truncate text-[11px] text-q-text-muted" title={rate.value.secondary}>
            {rate.value.secondary}
          </p>
        )}
        {rateLine?.stale && <p className="text-[11px] text-q-warning">{rateLine.text}</p>}
      </div>

      {(stats.some((capability) => capability !== null) || cacheTokens.length > 0) && (
        <div className="grid grid-cols-[repeat(auto-fit,minmax(min(100%,92px),1fr))] gap-x-3 gap-y-2.5 border-t border-q-border pt-3">
          {stats.map((capability) =>
            capability ? (
              <StatCell
                key={capability.capabilityId}
                label={capability.displayName}
                value={primaryText(capability)}
                missing={isMissing(capability)}
              />
            ) : null,
          )}
          {cacheTokens.map((capability) => (
            <StatCell
              key={capability.capabilityId}
              label={capability.displayName}
              value={primaryText(capability)}
              missing={false}
            />
          ))}
        </div>
      )}

      {staleAt ? <p className="text-[11px] text-q-warning">缓存 · 上次成功 {formatTime(staleAt)}</p> : null}
    </section>
  );
}

/**
 * DeepSeek 账号用量组合：资金概览（全宽）→ 模型/缓存双面板 → 消费趋势（全宽）。
 * 只渲染真实存在的能力；全部缺失时各面板展示「未获取」空轨道，不补零。
 */
export function DeepSeekUsageSummary({ capabilities }: { capabilities: CapabilitySnapshotViewModel[] }) {
  const balance = findCapability(capabilities, "balance");
  const financeSecondary = FINANCE_SECONDARY_IDS.map((id) => findCapability(capabilities, id)).filter(
    (capability): capability is CapabilitySnapshotViewModel => capability !== null,
  );
  const models = MODEL_IDS.map((id) => findCapability(capabilities, id));
  const trend = findCapability(capabilities, "usage_trend");
  const hasModels = models.some((capability) => capability !== null);
  const hasCache = findCapability(capabilities, "cache_hit_rate") !== null;
  return (
    <div className="flex min-w-0 flex-col gap-4">
      <section className="glass-panel flex min-w-0 flex-col gap-3 p-4">
        <div className="flex items-center justify-between gap-2">
          <div className="flex items-center gap-2">
            <Wallet size={15} aria-hidden className="text-q-text-muted" />
            <h3 className="text-[14px] font-semibold tracking-tight text-q-text-primary">资金概览</h3>
          </div>
          {balance && balance.freshness !== "fresh" && <FreshnessTag freshness={balance.freshness} />}
        </div>
        <div className="grid grid-cols-[repeat(auto-fit,minmax(min(100%,150px),1fr))] gap-x-4 gap-y-4 border-t border-q-border pt-3">
          <MoneyItem capability={balance} main />
          {financeSecondary.map((capability) => (
            <MoneyItem key={capability.capabilityId} capability={capability} />
          ))}
        </div>
      </section>

      {hasModels || hasCache ? (
        <div className="grid grid-cols-[repeat(auto-fit,minmax(min(100%,320px),1fr))] gap-4">
          {hasModels && (
            <section className="glass-panel flex min-w-0 flex-col gap-3 p-4">
              <div className="flex items-center gap-2">
                <BarChart3 size={15} aria-hidden className="text-q-text-muted" />
                <h3 className="text-[14px] font-semibold tracking-tight text-q-text-primary">模型用量</h3>
              </div>
              <div className="flex flex-col gap-2.5">
                {models.map((capability, index) => (
                  <ModelRow key={MODEL_IDS[index]} capability={capability} id={MODEL_IDS[index]} />
                ))}
              </div>
            </section>
          )}
          {hasCache && <CacheEfficiencyPanel capabilities={capabilities} />}
        </div>
      ) : null}

      {trend ? <UsageTrend capability={trend} /> : null}
    </div>
  );
}

/** 仅 DeepSeek 网页用量来源产出的能力；账号内命中任一即对该账号启用组合渲染。 */
const DEEPSEEK_ONLY_IDS: readonly string[] = [
  ...MODEL_IDS,
  ...STAT_IDS,
  ...CACHE_TOKEN_IDS,
  "cache_hit_rate",
  "usage_trend",
];

/** 参与资金概览组合的能力；total_spend 为可选第四项。 */
const FINANCE_IDS: readonly string[] = ["balance", ...FINANCE_SECONDARY_IDS];

/**
 * 账号能力拆分：命中 DeepSeek 专属能力时，把资金/模型/缓存/趋势相关能力拨入组合渲染，
 * 其余能力（如未来新增的额度窗口）仍走通用能力卡。
 */
export function splitDeepSeekUsageCapabilities(capabilities: CapabilitySnapshotViewModel[]): {
  composition: CapabilitySnapshotViewModel[];
  rest: CapabilitySnapshotViewModel[];
} {
  const hasComposition = capabilities.some((capability) => DEEPSEEK_ONLY_IDS.includes(capability.capabilityId));
  if (!hasComposition) {
    return { composition: [], rest: capabilities };
  }
  const compositionIds = new Set([...FINANCE_IDS, ...DEEPSEEK_ONLY_IDS]);
  return {
    composition: capabilities.filter((capability) => compositionIds.has(capability.capabilityId)),
    rest: capabilities.filter((capability) => !compositionIds.has(capability.capabilityId)),
  };
}
