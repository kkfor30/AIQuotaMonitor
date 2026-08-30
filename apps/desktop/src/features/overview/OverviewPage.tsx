import { useMemo, useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import {
  Boxes,
  ChevronRight,
  CircleAlert,
  CircleCheck,
  Info,
  RefreshCw,
  Settings2,
} from "lucide-react";
import { TrendLineChart, type TrendSeries } from "@/components/ui/TrendLineChart";
import { PlatformMark } from "@/features/platform-center/ProviderRail";
import { KeyPlatformWindow } from "@/features/platform-center/KeyPlatformWindow";
import {
  sortWindowCapabilities,
  windowShortLabel,
  windowTrendColor,
} from "@/features/platform-center/quota-windows";
import { compactPercentText, formatDateTime } from "@/lib/format";
import { fetchPlatformSummaries, refreshAllPlatforms } from "@/lib/ipc";
import { PLATFORM_SUMMARIES_QUERY_KEY } from "@/lib/query-client";
import { providerBrand } from "@/lib/provider-brand";
import { cn } from "@/lib/cn";
import type { PlatformCenterTarget } from "@/app/navigation";
import type {
  AccountSummaryViewModel,
  PlatformSummaryViewModel,
  SourceSummaryViewModel,
} from "@/lib/types";

/**
 * 总览（Apple Glass V6/V7 设计稿 01）：
 * 页头（总览 + 全局刷新 + 最后更新）→ 四段状态条 → 关键平台账号卡组 →
 * 需要关注 / 额度使用趋势 / 消费趋势 三卡行 → 最近刷新记录表格。
 * 全部数字来自真实 ViewModel；趋势缺序列显示空状态，不造数。
 */
export function OverviewPage({
  onOpenPlatform,
}: {
  onOpenPlatform: (target: PlatformCenterTarget) => void;
}) {
  const { data: platforms = [] } = useQuery({
    queryKey: PLATFORM_SUMMARIES_QUERY_KEY,
    queryFn: fetchPlatformSummaries,
  });

  const refreshMutation = useMutation({
    mutationFn: refreshAllPlatforms,
  });

  const connected = platforms.filter((p) => p.aggregateStatus !== "setup_required");
  const healthy = connected.filter((p) => p.aggregateStatus === "healthy").length;
  const partial = connected.filter((p) => p.aggregateStatus === "partial").length;
  const pending = platforms.filter(
    (p) => p.aggregateStatus === "setup_required" || p.aggregateStatus === "error",
  ).length;

  // 消费趋势：只纳入携带真实金额序列（usage_trend）的平台
  const consumptionSeries: TrendSeries[] = platforms.flatMap((platform) => {
    const capability = platform.capabilities.find(
      (item) => item.value.kind === "trend" && item.trend.length > 0,
    );
    if (!capability) return [];
    return [
      {
        id: platform.providerId,
        name: platform.displayName,
        color: providerBrand(platform.providerId).color,
        points: capability.trend.map((point) => ({ label: point.label, value: point.value })),
      },
    ];
  });

  // 额度使用趋势（V7）：进度条看剩余，趋势图看已使用（used = 100 - remaining，Rust 层已换算）。
  // 平台选择器只列存在真实窗口能力的平台；默认选第一个有真实窗口历史的平台。
  const windowPlatforms = useMemo(
    () =>
      platforms.filter((platform) =>
        platform.capabilities.some((capability) => capability.capabilityId.startsWith("quota_window_")),
      ),
    [platforms],
  );
  const [trendPlatformId, setTrendPlatformId] = useState<string | null>(null);
  const [trendAccountId, setTrendAccountId] = useState<string | null>(null);
  const trendPlatform =
    windowPlatforms.find((platform) => platform.providerId === trendPlatformId)
      ?? windowPlatforms.find((platform) =>
        platform.capabilities.some((capability) => capability.trend.length > 0),
      )
      ?? windowPlatforms[0]
      ?? null;
  const trendAccounts = useMemo(() => {
    if (!trendPlatform) return [] as AccountSummaryViewModel[];
    return trendPlatform.accounts.filter((account) =>
      trendPlatform.capabilities.some(
        (capability) =>
          capability.accountId === account.accountId
          && capability.capabilityId.startsWith("quota_window_"),
      ),
    );
  }, [trendPlatform]);
  const trendAccount =
    trendAccounts.find((account) => account.accountId === trendAccountId) ?? trendAccounts[0] ?? null;
  const usageSeries: TrendSeries[] =
    trendPlatform && trendAccount
      ? sortWindowCapabilities(
          trendPlatform.capabilities.filter(
            (capability) =>
              capability.accountId === trendAccount.accountId
              && capability.capabilityId.startsWith("quota_window_")
              && capability.trend.length > 0,
          ),
        )
          .slice(0, 4)
          .map((capability) => ({
            id: capability.capabilityId,
            name: `${windowShortLabel(capability)}已使用`,
            color: windowTrendColor(capability.capabilityId),
            points: capability.trend.map((point) => ({ label: point.label, value: point.value })),
          }))
      : [];

  const lastUpdatedAt = connected.reduce<number | null>((latest, platform) => {
    const sourceTime = platform.sources.reduce<number | null>((max, source) => {
      const at = source.lastSuccessAt ?? source.lastValidatedAt;
      return at !== null && (max === null || at > max) ? at : max;
    }, null);
    return sourceTime !== null && (latest === null || sourceTime > latest) ? sourceTime : latest;
  }, null);

  const refreshRows = useMemo(() => buildRefreshRows(platforms), [platforms]);

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-4 pt-2 pr-2">
      {/* 页头：总览 + 全局刷新 + 最后更新 */}
      <header className="flex items-center justify-between gap-4 px-1">
        <h1 className="text-[22px] font-bold tracking-tight text-q-text-primary">总览</h1>
        <div className="flex items-center gap-3">
          <button
            type="button"
            onClick={() => refreshMutation.mutate()}
            disabled={refreshMutation.isPending}
            className="inline-flex h-7 cursor-pointer items-center gap-1.5 rounded-q-pill border border-q-border bg-white/80 px-3 text-[12px] font-medium text-q-primary shadow-q-sm backdrop-blur transition-colors duration-150 hover:border-q-border-selected disabled:opacity-60"
          >
            <RefreshCw
              size={13}
              aria-hidden
              className={refreshMutation.isPending ? "animate-spin" : ""}
            />
            刷新
          </button>
          <span className="text-[11px] text-q-text-muted">
            最后更新：{lastUpdatedAt !== null ? formatFullTime(lastUpdatedAt) : "—"}
          </span>
        </div>
      </header>

      {/* 四段状态条 */}
      <section className="glass-panel flex flex-wrap items-center gap-x-10 gap-y-3 px-6 py-3.5">
        <StatusSegment
          icon={<Boxes size={17} aria-hidden />}
          tone="primary"
          value={platforms.length}
          label="个平台"
        />
        <StatusSegment
          icon={<CircleCheck size={17} aria-hidden />}
          tone="success"
          value={healthy}
          label="正常"
        />
        <StatusSegment
          icon={<CircleAlert size={17} aria-hidden />}
          tone="warning"
          value={partial}
          label="部分可用"
        />
        <StatusSegment
          icon={<Settings2 size={17} aria-hidden />}
          tone="danger"
          value={pending}
          label="待处理"
        />
      </section>

      {/* 关键平台：V7 账号卡组（叠卡/切换/手柄拖拽），「查看全部账户」进平台中心额度与用量 */}
      <KeyPlatformWindow platforms={platforms} onOpenPlatform={(providerId) => onOpenPlatform({ providerId, tab: "usage" })} />

      {/* 需要关注 + 窗口压力趋势 + 消费趋势 */}
      <div className="grid grid-cols-1 gap-4 xl:grid-cols-[minmax(280px,0.95fr)_minmax(0,1.25fr)_minmax(0,1fr)]">
        <AttentionCard platforms={platforms} onOpenPlatform={onOpenPlatform} />

        <section className="glass-panel flex flex-col gap-2.5 p-4">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <h2 className="text-[14px] font-semibold tracking-tight text-q-text-primary">
              额度使用趋势
              <span className="ml-1.5 text-[11px] font-normal text-q-text-muted">（最近 7 天）</span>
            </h2>
            <div className="flex flex-wrap items-center gap-2">
              <TrendSelect
                label="平台"
                value={trendPlatform?.providerId ?? ""}
                onChange={setTrendPlatformId}
                options={windowPlatforms.map((platform) => ({
                  value: platform.providerId,
                  label: platform.displayName,
                }))}
              />
              <TrendSelect
                label="账户"
                value={trendAccount?.accountId ?? ""}
                onChange={setTrendAccountId}
                options={trendAccounts.map((account) => ({
                  value: account.accountId,
                  label: account.displayName,
                }))}
              />
            </div>
          </div>
          <TrendLineChart
            series={usageSeries}
            valueKind="used_percent"
            yAxisLabel="已使用比例"
            emptyTitle="暂无额度使用历史"
            emptyDescription="刷新并积累真实窗口快照后，此处展示当前账号各窗口的已使用比例变化。"
          />
        </section>

        <section className="glass-panel flex flex-col gap-2.5 p-4">
          <div className="flex items-center justify-between gap-2">
            <h2 className="text-[14px] font-semibold tracking-tight text-q-text-primary">
              消费趋势
              <span className="ml-1.5 text-[11px] font-normal text-q-text-muted">（最近 7 天）</span>
            </h2>
            <span title="仅纳入提供官方金额序列的平台（如 DeepSeek 网页用量）">
              <Info size={14} aria-hidden className="cursor-help text-q-text-muted" />
            </span>
          </div>
          <TrendLineChart
            series={consumptionSeries}
            valueKind="money"
            emptyTitle="暂无消费趋势数据"
            emptyDescription="接入带官方金额序列的平台（如 DeepSeek 网页用量）后，此处展示每日消费变化。"
          />
        </section>
      </div>

      {/* 最近刷新记录：时间 / 平台 / 类型 / 结果 / 详情 */}
      <section className="glass-panel flex flex-col p-4">
        <h2 className="text-[14px] font-semibold tracking-tight text-q-text-primary">最近刷新记录</h2>
        <div className="mt-2.5 grid grid-cols-[170px_minmax(0,1fr)_120px_100px_minmax(0,1.5fr)] items-center gap-x-3 border-b border-q-border px-2 pb-2 text-[11px] font-medium text-q-text-muted">
          <span>时间</span>
          <span>平台</span>
          <span>类型</span>
          <span>结果</span>
          <span>详情</span>
        </div>
        <div className="flex flex-col">
          {refreshRows.length === 0 && (
            <p className="px-2 py-3 text-xs text-q-text-muted">暂无刷新记录</p>
          )}
          {refreshRows.map((row) => (
            <div
              key={row.key}
              className="grid grid-cols-[170px_minmax(0,1fr)_120px_100px_minmax(0,1.5fr)] items-center gap-x-3 border-b border-q-border/60 px-2 py-2 text-xs last:border-b-0"
            >
              <span className="truncate tabular-nums text-q-text-secondary">{row.time}</span>
              <span className="flex min-w-0 items-center gap-2">
                <PlatformMark providerId={row.providerId} size={20} />
                <span className="truncate font-medium text-q-text-primary">{row.platform}</span>
              </span>
              <span className="truncate text-q-text-secondary">{row.type}</span>
              <span className="flex items-center gap-1.5">
                <span
                  aria-hidden
                  className={cn(
                    "h-1.5 w-1.5 shrink-0 rounded-full",
                    row.ok ? "bg-q-success" : "bg-q-danger",
                  )}
                />
                <span className={row.ok ? "text-q-success-strong" : "text-q-danger"}>
                  {row.ok ? "成功" : "失败"}
                </span>
              </span>
              <span
                className="truncate text-q-text-secondary"
                title={row.detail}
                data-selectable="true"
              >
                {row.detail}
              </span>
            </div>
          ))}
        </div>
      </section>
    </div>
  );
}

function StatusSegment({
  icon,
  tone,
  value,
  label,
}: {
  icon: React.ReactNode;
  tone: "primary" | "success" | "warning" | "danger";
  value: number;
  label: string;
}) {
  const toneText =
    tone === "primary"
      ? "text-q-primary"
      : tone === "success"
        ? "text-q-success"
        : tone === "warning"
          ? "text-q-warning"
          : "text-q-danger";
  const toneBg =
    tone === "primary"
      ? "bg-q-primary-soft"
      : tone === "success"
        ? "bg-q-success-soft"
        : tone === "warning"
          ? "bg-q-warning-soft"
          : "bg-q-danger-soft";
  return (
    <div className="flex items-center gap-2.5">
      <span
        aria-hidden
        className={cn("flex h-8 w-8 shrink-0 items-center justify-center rounded-[10px]", toneBg, toneText)}
      >
        {icon}
      </span>
      <span className={cn("text-[20px] font-bold leading-none tabular-nums", toneText)}>
        {value}
      </span>
      <span className="text-[12px] text-q-text-secondary">{label}</span>
    </div>
  );
}

/** 趋势卡筛选下拉：紧凑行内样式，选项全部来自真实 ViewModel。 */
function TrendSelect({
  label,
  value,
  onChange,
  options,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  options: Array<{ value: string; label: string }>;
}) {
  if (options.length === 0) return null;
  return (
    <label className="inline-flex items-center gap-1.5 text-[11px] text-q-text-muted">
      {label}
      <select
        value={value}
        onChange={(event) => onChange(event.target.value)}
        className="h-7 max-w-[130px] cursor-pointer rounded-q-control border border-q-border bg-q-surface px-2 text-[12px] font-medium text-q-text-primary outline-none focus:border-q-primary"
      >
        {options.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
    </label>
  );
}

type AttentionRow = {
  key: string;
  providerId: string;
  platform: string;
  title: string;
  detail: string;
  target: PlatformCenterTarget;
};

function AttentionCard({
  platforms,
  onOpenPlatform,
}: {
  platforms: PlatformSummaryViewModel[];
  onOpenPlatform: (target: PlatformCenterTarget) => void;
}) {
  const rows: AttentionRow[] = [];
  for (const platform of platforms) {
    if (platform.aggregateStatus === "error" || platform.aggregateStatus === "partial") {
      for (const source of platform.sources) {
        if (source.state === "error" || source.state === "auth_required") {
          rows.push({
            key: `${platform.providerId}-${source.sourceId}`,
            providerId: platform.providerId,
            platform: platform.displayName,
            title: source.accountName,
            detail:
              source.errorMessage ??
              (source.state === "auth_required" ? "凭据待配置，刷新暂停" : "刷新失败"),
            target: {
              providerId: platform.providerId,
              tab: "sources",
              focusSourceId: source.sourceId,
            },
          });
        }
      }
    } else if (platform.aggregateStatus === "setup_required") {
      rows.push({
        key: platform.providerId,
        providerId: platform.providerId,
        platform: platform.displayName,
        title: "未接入",
        detail: "配置数据来源后纳入监控",
        target: { providerId: platform.providerId, tab: "sources" },
      });
    }
    // 低额度窗口只陈述中性事实（V7）：「平台 · 账号 · 7天剩余 0%」，不附加任何指令文案
    for (const capability of sortWindowCapabilities(
      platform.capabilities.filter(
        (item) =>
          item.capabilityId.startsWith("quota_window_")
          && item.value.primary !== null
          && item.freshness !== "missing",
      ),
    )) {
      const remaining = Number.parseFloat(capability.value.primary!.replace("%", ""));
      if (!Number.isFinite(remaining) || remaining >= 20) continue;
      const accountName =
        platform.accounts.find((account) => account.accountId === capability.accountId)?.displayName ?? "";
      rows.push({
        key: `${platform.providerId}-${capability.accountId}-${capability.capabilityId}`,
        providerId: platform.providerId,
        platform: platform.displayName,
        title: accountName,
        detail: `${windowShortLabel(capability)}剩余 ${compactPercentText(capability.value.primary!)}`,
        target: { providerId: platform.providerId, tab: "usage" },
      });
    }
  }

  return (
    <section className="glass-panel flex min-h-[220px] flex-col p-4">
      <h2 className="text-[14px] font-semibold tracking-tight text-q-text-primary">需要关注</h2>
      <div className="mt-1 flex flex-col">
        {rows.length === 0 && (
          <div className="flex items-center gap-2.5 px-1 py-3">
            <CircleCheck size={16} className="shrink-0 text-q-success" aria-hidden />
            <p className="text-xs text-q-text-secondary">全部平台运行正常。</p>
          </div>
        )}
        {rows.map((row) => (
          <button
            key={row.key}
            type="button"
            onClick={() => onOpenPlatform(row.target)}
            className="group flex cursor-pointer items-center gap-2.5 border-b border-q-border/60 px-1 py-2.5 text-left last:border-b-0 hover:bg-q-primary-softer/60"
          >
            <PlatformMark providerId={row.providerId} size={32} />
            <div className="min-w-0 flex-1">
              <p className="truncate text-[13px] font-semibold text-q-text-primary">
                {row.platform}
                <span className="ml-1.5 font-normal text-q-text-secondary">{row.title}</span>
              </p>
              <p className="mt-0.5 truncate text-[11px] text-q-text-muted" title={row.detail}>
                {row.detail}
              </p>
            </div>
            <ChevronRight
              size={15}
              aria-hidden
              className="shrink-0 text-q-text-muted transition-colors group-hover:text-q-primary"
            />
          </button>
        ))}
      </div>
    </section>
  );
}

type RefreshRow = {
  key: string;
  time: string;
  providerId: string;
  platform: string;
  type: string;
  ok: boolean;
  detail: string;
};

/** 从真实来源快照组装刷新表格行：类型由来源能力推导，详情取代表能力值。 */
function buildRefreshRows(platforms: PlatformSummaryViewModel[]): RefreshRow[] {
  const rows: Array<RefreshRow & { at: number }> = [];
  for (const platform of platforms) {
    if (platform.aggregateStatus === "setup_required") continue;
    for (const source of platform.sources) {
      const at = source.lastSuccessAt ?? source.lastValidatedAt;
      if (at === null) continue;
      rows.push({
        key: `${platform.providerId}-${source.sourceId}`,
        at,
        time: formatDateTime(at),
        providerId: platform.providerId,
        platform: platform.displayName,
        type: refreshTypeLabel(source),
        ok: source.state === "ready",
        detail: refreshDetail(platform, source),
      });
    }
  }
  return rows
    .sort((left, right) => right.at - left.at)
    .slice(0, 8)
    .map(({ at: _at, ...row }) => row);
}

function refreshTypeLabel(source: SourceSummaryViewModel): string {
  const caps = source.capabilityIds;
  if (caps.some((id) => id.startsWith("quota_window"))) return "窗口刷新";
  if (caps.includes("balance") || caps.includes("month_spend")) return "余额刷新";
  if (caps.includes("usage_trend")) return "用量同步";
  return "状态检查";
}

function refreshDetail(platform: PlatformSummaryViewModel, source: SourceSummaryViewModel): string {
  const caps = platform.capabilities.filter(
    (capability) => capability.sourceId === source.sourceId && capability.value.primary !== null,
  );
  const windowCap = caps
    .filter((item) => item.capabilityId.startsWith("quota_window_") && item.value.primary)
    .sort((left, right) => left.capabilityId.localeCompare(right.capabilityId))[0];
  if (windowCap?.value.primary) {
    const label =
      windowCap.capabilityId === "quota_window_30d"
        ? "30天窗口"
        : windowCap.capabilityId === "quota_window_7d"
          ? "7天窗口"
          : windowCap.capabilityId === "quota_window_5h"
            ? "5小时窗口"
            : windowCap.displayName.split("·").at(-1)?.trim() || "窗口";
    return `${label}：${compactPercentText(windowCap.value.primary)}`;
  }
  const order = ["balance", "month_spend", "cache_hit_rate"];
  for (const id of order) {
    const capability = caps.find((item) => item.capabilityId === id);
    if (capability?.value.primary) {
      const label =
        id === "balance" ? "余额" : id === "month_spend" ? "本月消费" : "缓存命中率";
      return `${label}：${compactPercentText(capability.value.primary)}`;
    }
  }
  if (source.state === "error" && source.errorMessage) return source.errorMessage;
  return "—";
}

function formatFullTime(epochMs: number): string {
  const date = new Date(epochMs);
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`;
}
