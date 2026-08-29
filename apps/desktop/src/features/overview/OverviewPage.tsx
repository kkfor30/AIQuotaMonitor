import { useQuery } from "@tanstack/react-query";
import { AlertTriangle, ArrowRight, CircleCheck, ShieldCheck } from "lucide-react";
import { TrendLineChart, type TrendSeries } from "@/components/ui/TrendLineChart";
import { KeyPlatformWindow } from "@/features/platform-center/KeyPlatformWindow";
import { formatTime } from "@/lib/format";
import { fetchPlatformSummaries } from "@/lib/ipc";
import { PLATFORM_SUMMARIES_QUERY_KEY } from "@/lib/query-client";
import { providerBrand } from "@/lib/provider-brand";
import { cn } from "@/lib/cn";
import type { PlatformCenterTarget } from "@/app/navigation";

/**
 * 总览（Apple Glass V6）：
 * 只回答「当前整体是否健康、哪里需要处理、哪个窗口值得关注」。
 * - 关键平台：固定高度横向窗口，支持拖拽排序（复用平台排序持久化）
 * - 窗口压力趋势 / 消费趋势：独立折线图卡，只读真实历史，缺序列显示空状态
 * - 需要关注与最近刷新直达平台中心对应位置
 */
export function OverviewPage({
  onOpenPlatform,
  onOpenRadar,
}: {
  onOpenPlatform: (target: PlatformCenterTarget) => void;
  onOpenRadar: () => void;
}) {
  const { data: platforms = [] } = useQuery({
    queryKey: PLATFORM_SUMMARIES_QUERY_KEY,
    queryFn: fetchPlatformSummaries,
  });

  const connected = platforms.filter((p) => p.aggregateStatus !== "setup_required");
  const needsAttention = connected.filter(
    (p) => p.aggregateStatus === "partial" || p.aggregateStatus === "error",
  );
  const notSetup = platforms.filter((p) => p.aggregateStatus === "setup_required");

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

  // 窗口压力趋势：只纳入 5 小时 / 7 天 Token Plan 窗口能力的真实历史
  const pressureSeries: TrendSeries[] = platforms.flatMap((platform) =>
    platform.capabilities
      .filter(
        (item) =>
          (item.capabilityId === "quota_window_5h" || item.capabilityId === "quota_window_7d") &&
          item.trend.length > 0,
      )
      .map((item) => ({
        id: `${platform.providerId}:${item.capabilityId}`,
        name: `${platform.displayName} · ${item.displayName}`,
        color: providerBrand(platform.providerId).color,
        points: item.trend.map((point) => ({ label: point.label, value: point.value })),
      })),
  );

  const lastRefreshAt = connected.reduce<number | null>((latest, platform) => {
    const sourceTime = platform.sources.reduce<number | null>((max, source) => {
      const at = source.lastSuccessAt ?? source.lastValidatedAt;
      return at !== null && (max === null || at > max) ? at : max;
    }, null);
    return sourceTime !== null && (latest === null || sourceTime > latest) ? sourceTime : latest;
  }, null);

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-4 pt-2 pr-2">
      {/* 页面标题 */}
      <header className="flex items-end justify-between gap-4 px-1">
        <div>
          <h1 className="text-[20px] font-semibold tracking-tight text-q-text-primary">总览</h1>
          <p className="mt-0.5 text-[13px] text-q-text-secondary">
            全部平台的健康、额度与消费摘要
          </p>
        </div>
      </header>

      {/* 关键平台：固定高度横向窗口 */}
      <KeyPlatformWindow
        platforms={platforms}
        onOpenPlatform={(providerId) => onOpenPlatform({ providerId, tab: "usage" })}
      />

      {/* 双趋势图：窗口压力 + 消费 */}
      <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
        <section className="glass-panel flex flex-col gap-3 p-4">
          <div className="flex items-start justify-between gap-3">
            <div>
              <h2 className="text-[15px] font-semibold tracking-tight text-q-text-primary">
                窗口压力趋势
              </h2>
              <p className="mt-0.5 text-[11px] text-q-text-muted">
                5 小时 / 7 天 Token Plan 窗口用量历史
              </p>
            </div>
            <button
              type="button"
              onClick={onOpenRadar}
              className="inline-flex shrink-0 cursor-pointer items-center gap-1 rounded-q-control px-2 py-1 text-[12px] font-medium text-q-primary transition-colors duration-150 hover:bg-q-primary-softer"
            >
              前往 GPT 重置雷达
              <ArrowRight size={13} aria-hidden />
            </button>
          </div>
          <TrendLineChart
            series={pressureSeries}
            valueKind="percent"
            emptyTitle="暂无窗口压力历史序列"
            emptyDescription="接入 5 小时 / 7 天 Token Plan 平台并产生历史快照后，此处展示各窗口的用量变化。"
          />
        </section>

        <section className="glass-panel flex flex-col gap-3 p-4">
          <div className="flex items-start justify-between gap-3">
            <div>
              <h2 className="text-[15px] font-semibold tracking-tight text-q-text-primary">
                消费趋势
              </h2>
              <p className="mt-0.5 text-[11px] text-q-text-muted">近 7 日真实金额消费</p>
            </div>
          </div>
          <TrendLineChart
            series={consumptionSeries}
            valueKind="money"
            emptyTitle="暂无消费趋势数据"
            emptyDescription="接入带官方金额序列的平台（如 DeepSeek 网页用量）后，此处展示每日消费变化。"
          />
        </section>
      </div>

      {/* 整体状态 + 需要关注 + 最近刷新 */}
      <div className="grid grid-cols-1 gap-4 xl:grid-cols-[280px_minmax(0,1fr)_340px]">
        <section className="glass-panel flex flex-col gap-3 p-4">
          <h2 className="text-[15px] font-semibold tracking-tight text-q-text-primary">整体状态</h2>
          <div className="flex items-center gap-3">
            <span
              aria-hidden
              className={cn(
                "flex h-11 w-11 shrink-0 items-center justify-center rounded-[14px]",
                connected.length === 0
                  ? "bg-q-neutral-soft text-q-neutral"
                  : needsAttention.length === 0
                    ? "bg-q-success-soft text-q-success"
                    : "bg-q-warning-soft text-q-warning",
              )}
            >
              {connected.length > 0 && needsAttention.length === 0 ? (
                <ShieldCheck size={22} aria-hidden />
              ) : (
                <AlertTriangle size={22} aria-hidden />
              )}
            </span>
            <div className="min-w-0">
              <p className="text-[15px] font-semibold text-q-text-primary">
                {connected.length === 0
                  ? "尚未接入平台"
                  : needsAttention.length === 0
                    ? "全部正常"
                    : `${needsAttention.length} 个平台需要处理`}
              </p>
              <p className="mt-0.5 text-[11px] text-q-text-muted">
                已接入 {connected.length} / {platforms.length} · 未接入 {notSetup.length}
              </p>
            </div>
          </div>
          <p className="text-[11px] leading-relaxed text-q-text-muted">
            {lastRefreshAt !== null
              ? `最近刷新 ${formatTime(lastRefreshAt)}`
              : "暂无刷新记录，接入来源后自动汇总。"}
          </p>
        </section>

        {/* 需要关注：点击直达平台中心对应位置 */}
        <section className="glass-panel flex flex-col gap-3 p-4">
          <h2 className="text-[15px] font-semibold tracking-tight text-q-text-primary">需要关注</h2>
          <div className="flex flex-col gap-2.5">
            {needsAttention.length === 0 && notSetup.length === 0 && (
              <p className="py-2 text-[13px] text-q-text-secondary">全部平台运行正常。</p>
            )}
            {needsAttention.map((platform) =>
              platform.sources
                .filter((source) => source.state === "error" || source.state === "auth_required")
                .map((source) => (
                  <AttentionItem
                    key={`${platform.providerId}-${source.sourceId}`}
                    tone={source.state === "error" ? "danger" : "warning"}
                    title={`${platform.displayName} · ${source.displayName}`}
                    detail={
                      source.errorMessage ??
                      (source.state === "auth_required" ? "凭据待配置" : "刷新失败")
                    }
                    actionText="去处理"
                    onClick={() =>
                      onOpenPlatform({
                        providerId: platform.providerId,
                        tab: "sources",
                        focusSourceId: source.sourceId,
                      })
                    }
                  />
                )),
            )}
            {notSetup.map((platform) => (
              <AttentionItem
                key={platform.providerId}
                tone="neutral"
                title={`${platform.displayName} 未接入`}
                detail="配置来源后纳入监控"
                actionText="去接入"
                onClick={() =>
                  onOpenPlatform({ providerId: platform.providerId, tab: "sources" })
                }
              />
            ))}
          </div>
        </section>

        {/* 最近刷新（跨平台聚合，无数据的时间戳跳过） */}
        <section className="glass-panel flex flex-col gap-2.5 p-4">
          <h2 className="text-[15px] font-semibold tracking-tight text-q-text-primary">最近刷新</h2>
          <ul className="flex flex-col gap-2">
            {connected.flatMap((platform) =>
              platform.sources
                .filter(
                  (source) => source.lastSuccessAt !== null || source.lastValidatedAt !== null,
                )
                .map((source) => {
                  const ok = source.state === "ready";
                  const at = ok ? source.lastSuccessAt : source.lastValidatedAt;
                  return (
                    <li
                      key={`${platform.providerId}-${source.sourceId}`}
                      className="flex items-start gap-2 text-xs"
                    >
                      {ok ? (
                        <CircleCheck
                          size={14}
                          className="mt-0.5 shrink-0 text-q-success"
                          aria-hidden
                        />
                      ) : (
                        <AlertTriangle
                          size={14}
                          className="mt-0.5 shrink-0 text-q-warning"
                          aria-hidden
                        />
                      )}
                      <span className="min-w-0 flex-1 truncate text-q-text-primary">
                        {platform.displayName} · {source.displayName}
                      </span>
                      <span className="shrink-0 text-q-text-muted">{formatTime(at)}</span>
                    </li>
                  );
                }),
            )}
            {connected.length === 0 && (
              <li className="text-xs text-q-text-muted">暂无刷新记录</li>
            )}
          </ul>
        </section>
      </div>
    </div>
  );
}

function AttentionItem({
  tone,
  title,
  detail,
  actionText,
  onClick,
}: {
  tone: "danger" | "warning" | "neutral";
  title: string;
  detail: string;
  actionText: string;
  onClick: () => void;
}) {
  const toneClass =
    tone === "danger"
      ? "text-q-danger"
      : tone === "warning"
        ? "text-q-warning"
        : "text-q-neutral";
  return (
    <div className="flex items-start gap-2.5 border-t border-q-border pt-2.5 first:border-t-0 first:pt-0">
      <AlertTriangle size={15} className={`mt-0.5 shrink-0 ${toneClass}`} aria-hidden />
      <div className="min-w-0 flex-1">
        <p className="truncate text-[13px] font-medium text-q-text-primary">{title}</p>
        <p className={`mt-0.5 truncate text-xs ${toneClass}`}>{detail}</p>
      </div>
      <button
        type="button"
        onClick={onClick}
        className="inline-flex shrink-0 cursor-pointer items-center gap-0.5 rounded-q-control px-2 py-1 text-xs font-medium text-q-primary transition-colors duration-150 hover:bg-q-primary-softer"
      >
        {actionText}
        <ArrowRight size={12} aria-hidden />
      </button>
    </div>
  );
}
