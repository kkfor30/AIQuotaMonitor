import { useQuery } from "@tanstack/react-query";
import { AlertTriangle, ArrowRight, CircleCheck, Plus } from "lucide-react";
import { AggregateStatusBadge } from "@/components/ui/StatusBadge";
import { EmptyState } from "@/components/ui/EmptyState";
import { PlatformMark } from "@/features/platform-center/ProviderRail";
import { UsageTrend } from "@/features/platform-center/UsageTrend";
import { formatTime } from "@/lib/format";
import { fetchPlatformSummaries } from "@/lib/ipc";
import { PLATFORM_SUMMARIES_QUERY_KEY } from "@/lib/query-client";
import type { PlatformCenterTarget } from "@/app/navigation";
import {
  AGGREGATE_STATUS_META,
  type PlatformSummaryViewModel,
} from "@/lib/types";

/**
 * 总览（product-shell-v5）：
 * 只回答「当前整体是否健康、哪里需要处理、哪个窗口值得关注」。
 * 不承担凭据配置，不复制平台中心完整详情。
 * 所有数字由静态 ViewModel 推导，缺失能力显示未接入而非示例值。
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
  const deepseek = platforms.find((p) => p.providerId === "deepseek");
  const trendCapability = deepseek?.capabilities.find(
    (c) => c.capabilityId === "usage_trend",
  );

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto pr-1">
      {/* 全局状态汇总 */}
      <section className="glass-panel flex flex-wrap items-center gap-x-8 gap-y-3 px-5 py-4">
        <GlobalStat
          ok={needsAttention.length === 0}
          label="整体状态"
          value={
            connected.length === 0
              ? "尚未接入平台"
              : needsAttention.length === 0
                ? "全部正常"
                : `${needsAttention.length} 个平台需要处理`
          }
        />
        <GlobalStat label="已接入平台" value={`${connected.length} / ${platforms.length}`} />
        <GlobalStat label="未接入" value={`${notSetup.length}`} />
        <p className="ml-auto max-w-xs text-right text-[11px] leading-relaxed text-q-text-muted">
          阶段一为静态演示数据；接入真实 Source 后此处汇总各平台刷新结果。
        </p>
      </section>

      <div className="grid min-h-0 flex-1 grid-cols-[minmax(0,1fr)_320px] gap-4">
        <div className="flex min-w-0 flex-col gap-4">
          {/* 关键平台卡片（已接入平台的主能力摘要） */}
          <section className="flex flex-col gap-2">
            <SectionTitle>关键平台</SectionTitle>
            {connected.length === 0 ? (
              <EmptyState
                title="暂无已接入平台"
                description="接入平台后此处展示余额、消费与窗口压力摘要。"
              />
            ) : (
              <div className="grid grid-cols-2 gap-3">
                {connected.map((platform) => (
                  <KeyPlatformCard
                    key={platform.providerId}
                    platform={platform}
                    onOpen={() =>
                      onOpenPlatform({ providerId: platform.providerId, tab: "usage" })
                    }
                  />
                ))}
              </div>
            )}
          </section>

          {/* 消费趋势（复用 DeepSeek 静态趋势，stale 标记保留） */}
          {trendCapability && trendCapability.trend.length > 0 && (
            <section className="flex flex-col gap-2">
              <SectionTitle>消费趋势</SectionTitle>
              <UsageTrend capability={trendCapability} />
            </section>
          )}

          {/* 窗口压力：GPT/Codex 能力尚未接入，明确显示未提供而非编造数字 */}
          <section className="flex flex-col gap-2">
            <SectionTitle>窗口压力</SectionTitle>
            <div className="glass-panel flex items-center justify-between gap-3 px-4 py-3">
              <p className="text-[13px] text-q-text-secondary">
                GPT / Codex 5 小时窗口用量与重置倒计时
              </p>
              <button
                type="button"
                onClick={onOpenRadar}
                className="inline-flex cursor-pointer items-center gap-1 text-[13px] font-medium text-q-primary hover:underline"
              >
                前往 GPT 重置雷达 <ArrowRight size={14} aria-hidden />
              </button>
            </div>
            <p className="px-1 text-[11px] text-q-text-muted">未接入：窗口压力数据待 GPT Source 接入后提供。</p>
          </section>
        </div>

        <div className="flex min-w-0 flex-col gap-4">
          {/* 需要关注：点击直达平台中心对应位置 */}
          <section className="glass-panel flex flex-col gap-3 p-4">
            <SectionTitle className="px-0">需要关注</SectionTitle>
            {needsAttention.length === 0 && notSetup.length === 0 && (
              <p className="text-[13px] text-q-text-secondary">全部平台运行正常。</p>
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
          </section>

          {/* 最近刷新（跨平台聚合，无数据的时间戳跳过） */}
          <section className="glass-panel flex flex-1 flex-col gap-2.5 p-4">
            <SectionTitle className="px-0">最近刷新</SectionTitle>
            <ul className="flex flex-col gap-2">
              {connected.flatMap((platform) =>
                platform.sources
                  .filter((source) => source.lastSuccessAt !== null || source.lastValidatedAt !== null)
                  .map((source) => {
                    const ok = source.state === "ready";
                    const at = ok ? source.lastSuccessAt : source.lastValidatedAt;
                    return (
                      <li
                        key={`${platform.providerId}-${source.sourceId}`}
                        className="flex items-start gap-2 text-xs"
                      >
                        {ok ? (
                          <CircleCheck size={14} className="mt-0.5 shrink-0 text-q-success" aria-hidden />
                        ) : (
                          <AlertTriangle size={14} className="mt-0.5 shrink-0 text-q-warning" aria-hidden />
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
    </div>
  );
}

function GlobalStat({ label, value, ok }: { label: string; value: string; ok?: boolean }) {
  return (
    <div className="flex flex-col gap-0.5">
      <span className="text-[11px] text-q-text-muted">{label}</span>
      <span
        className={`text-[15px] font-semibold ${
          ok === undefined ? "text-q-text-primary" : ok ? "text-q-success-strong" : "text-q-warning"
        }`}
      >
        {value}
      </span>
    </div>
  );
}

function SectionTitle({ children, className = "" }: { children: React.ReactNode; className?: string }) {
  return <p className={`px-1 text-[13px] font-medium text-q-text-secondary ${className}`}>{children}</p>;
}

function KeyPlatformCard({
  platform,
  onOpen,
}: {
  platform: PlatformSummaryViewModel;
  onOpen: () => void;
}) {
  const balance = platform.capabilities.find(
    (c) => c.capabilityId === "balance" && c.value.primary !== null,
  );
  const month = platform.capabilities.find((c) => c.capabilityId === "month_spend");
  return (
    <button
      type="button"
      onClick={onOpen}
      className="glass-panel flex cursor-pointer flex-col gap-3 p-4 text-left transition-colors duration-150 hover:border-q-border-selected"
    >
      <div className="flex items-center gap-3">
        <PlatformMark providerId={platform.providerId} size={32} />
        <span className="flex-1 truncate text-sm font-medium text-q-text-primary">
          {platform.displayName}
        </span>
        <AggregateStatusBadge status={platform.aggregateStatus} />
      </div>
      <div className="grid grid-cols-2 gap-2 text-xs">
        <div>
          <p className="text-q-text-muted">余额</p>
          <p className="mt-0.5 font-semibold text-q-text-primary" data-selectable="true">
            {balance ? balance.value.primary : "未提供"}
          </p>
        </div>
        <div>
          <p className="text-q-text-muted">本月消费</p>
          <p className="mt-0.5 font-semibold text-q-text-primary" data-selectable="true">
            {month && month.value.primary ? month.value.primary : "未提供"}
          </p>
        </div>
      </div>
      <span className="inline-flex items-center gap-1 text-[11px] text-q-text-muted">
        {AGGREGATE_STATUS_META[platform.aggregateStatus].label} · 点击查看详情
      </span>
    </button>
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
    <div className="flex items-start gap-2.5 border-t border-q-border pt-3 first:border-t-0 first:pt-0">
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
        {tone === "neutral" ? <Plus size={12} aria-hidden /> : <ArrowRight size={12} aria-hidden />}
      </button>
    </div>
  );
}
