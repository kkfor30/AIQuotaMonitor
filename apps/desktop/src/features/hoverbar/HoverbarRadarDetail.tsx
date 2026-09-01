/**
 * 悬浮详情内的 GPT 重置雷达二级页（生命周期 V1）。
 * 固定顺序：当前事件 → 两路判断（CodexRadar / AI）→ 本机额度验证 → 事件时间线 → 事件相关动态。
 * 三路证据互不覆盖：来源行不受 AI 开关影响，额度不可达只表示无法验证。
 * 同一页面兼容四边停靠：宽停靠完整展示，窄停靠由 CSS 压缩次要信息（不缩字号、不裁按钮）。
 */
import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { ArrowLeft, ExternalLink, Languages, RefreshCw } from "lucide-react";
import {
  ipcErrorMessage,
  openExternalUrl,
  translateRadarPost,
  type QuotaVerification,
  type RadarPost,
  type RadarSnapshot,
} from "@/lib/ipc";
import { RADAR_SNAPSHOT_QUERY_KEY } from "@/lib/query-client";
import {
  formatHoverbarClock,
  formatRadarRangeLabel,
  humanizeRadarPostRefs,
  quotaBadgeLabel,
  quotaCorrelationLabel,
  radarEventStatusSummary,
  radarObservationPeriodLabel,
  radarPhaseLabel,
  radarTemporalLabel,
  shouldShowRadarTemporalBadge,
} from "./hoverbar-state";

const POST_BADGE_LABEL: Record<string, string> = {
  RESET: "重置相关",
  BANKED: "已落地",
  LIMITS: "限制",
  VERIFYING: "重置相关",
  NOTE: "动态",
  reset_related: "重置相关",
  reset_announcement: "重置公告",
  无重置信号: "无重置信号",
  间接相关: "间接相关",
  重置相关: "重置相关",
  重置公告: "重置公告",
};

export function HoverbarRadarDetail({
  radar,
  onBack,
  onRefresh,
  refreshing = false,
  refreshError = null,
  onCancel,
  onRetryQuota,
  quotaRefreshing = false,
}: {
  radar: RadarSnapshot | undefined;
  onBack: () => void;
  onRefresh?: () => void;
  refreshing?: boolean;
  refreshError?: string | null;
  onCancel?: () => void;
  onRetryQuota?: () => void;
  quotaRefreshing?: boolean;
}) {
  const queryClient = useQueryClient();
  const translate = useMutation({
    mutationFn: (postId: string) => translateRadarPost(postId),
    onSuccess: (snapshot) => queryClient.setQueryData(RADAR_SNAPSHOT_QUERY_KEY, snapshot),
  });

  const event = radar?.event ?? null;
  const phase = radarPhaseLabel(event?.phase);
  const observationPeriod = event ? radarObservationPeriodLabel(event) : null;
  const knownPosts = radar?.posts ?? [];
  const source = radar?.sourceAssessment;
  const aiLines = aiAssessmentLines(radar?.aiAssessment);
  const verifications = radar?.quotaVerifications ?? [];
  const eventPosts = event
    ? (radar?.posts ?? []).filter((post) => event.postIds.includes(post.id))
    : (radar?.posts ?? []).slice(0, 3);

  // 分析范围为只读标签：修改入口在主窗口 AI 辅助分析页，悬浮页不提供第二套选择器。
  const rangeKey = radar?.analysisPrefs.rangeKey;
  const rangeTitle =
    radar?.analysisPrefs.analyze
      ? "当前检查将使用主窗口中保存的分析范围"
      : "仅影响启用 AI 后的分析输入；来源公告与帖子同步不受范围限制";

  const sourceText =
    source?.headline ?? radar?.notice?.headline ?? radar?.latest?.summary ?? radar?.latest?.translatedText ?? radar?.latest?.text ?? "暂未同步来源内容";
  const sourceSub = source?.lead ?? null;
  const sourceFreshness =
    source?.lastSyncedAt == null
      ? "尚未同步"
      : `${formatHoverbarClock(source.lastSyncedAt)}${source.freshness === "stale" ? " · 缓存可能过期" : ""}`;

  return (
    <div className="hb-radar-page">
      <div className="hb-radar-top">
        <button type="button" className="hb-radar-back" onClick={onBack}>
          <ArrowLeft size={14} aria-hidden />
          返回额度
        </button>
        <span className="hb-radar-range" title={rangeTitle}>
          <span className="hb-radar-range-full">分析范围：{formatRadarRangeLabel(rangeKey)}</span>
          <span className="hb-radar-range-compact">{formatRadarRangeLabel(rangeKey, true)}</span>
        </span>
        {onRefresh ? (
          <button
            type="button"
            className="hb-radar-refresh"
            onClick={refreshing && onCancel ? onCancel : onRefresh}
            data-loading={refreshing || undefined}
            aria-label={refreshing ? "终止检查" : "刷新重置信号"}
            title={refreshing ? "终止检查" : "刷新重置信号"}
          >
            <RefreshCw size={13} aria-hidden className={refreshing ? "hb-spin" : ""} />
            {refreshing ? "终止" : "刷新"}
          </button>
        ) : null}
        <span className="hb-radar-pill">仅为推测</span>
      </div>

      {/* 1. 当前重置事件 */}
      <section className="hb-radar-card">
        <div className="hb-radar-card-head">
          <h3 className="hb-radar-card-title">当前重置事件</h3>
          {event && phase ? (
            <span className="radar-phase-badge" data-phase={event.phase}>
              {phase}
            </span>
          ) : null}
        </div>
        {event ? (
          <>
            <p className="hb-radar-field-label">结论</p>
            <p className="hb-radar-text" data-selectable="true">
              {humanizeRadarPostRefs(event.title, knownPosts)}
            </p>
            {observationPeriod ? (
              <p className="hb-radar-meta" data-observation="true">
                {observationPeriod}
              </p>
            ) : null}
            <p className="hb-radar-field-label">状态说明</p>
            <p className="hb-radar-meta" data-selectable="true">
              {radarEventStatusSummary(event)}
            </p>
            {(() => {
              const temporal = radarTemporalLabel(event.temporalStatus);
              return temporal && shouldShowRadarTemporalBadge(event.phase, event.temporalStatus) ? (
                <p className="hb-radar-meta" data-temporal={event.temporalStatus}>
                  {temporal}
                  {event.expectedAt ? ` · 预告 ${formatHoverbarClock(event.expectedAt)}` : ""}
                </p>
              ) : null;
            })()}
            <p className="hb-radar-meta">
              首帖发布 {formatHoverbarClock(event.firstSignalAt)} · 最新证据 {formatHoverbarClock(event.latestEvidenceAt)}
            </p>
          </>
        ) : (
          <p className="hb-radar-meta">暂无进行中的重置事件。出现强信号并开启 AI 分析后，会自动建立事件。</p>
        )}
      </section>

      {/* 2. 两路判断 */}
      <section className="hb-radar-card">
        <h3 className="hb-radar-card-title">两路判断</h3>
        <div className="hb-radar-judge">
          <span className="hb-radar-strip-tag">CodexRadar</span>
          <div className="hb-radar-judge-body">
            <p className="hb-radar-text" data-selectable="true">
              {sourceText}
            </p>
            {sourceSub ? (
              <p className="hb-radar-meta" data-selectable="true">
                {sourceSub}
              </p>
            ) : null}
            <p className="hb-radar-meta">{sourceFreshness}</p>
          </div>
        </div>
        <div className="hb-radar-judge">
          <span className="hb-radar-strip-tag">AI分析</span>
          <div className="hb-radar-judge-body">
            <p className="hb-radar-text" data-selectable="true">
              {humanizeRadarPostRefs(aiLines.primary, knownPosts)}
            </p>
            {aiLines.secondary ? (
              <p className="hb-radar-meta" data-selectable="true">
                {aiLines.secondary}
              </p>
            ) : null}
          </div>
        </div>
      </section>

      {/* 3. 本机额度验证 */}
      <section className="hb-radar-card">
        <h3 className="hb-radar-card-title">本机额度验证</h3>
        {verifications.length === 0 ? (
          <p className="hb-radar-meta">未接入 GPT 额度来源。</p>
        ) : (
          verifications.map((item) => <QuotaVerificationRow key={item.sourceId} item={item} />)
        )}
        {onRetryQuota ? (
          <button
            type="button"
            className="hb-quota-retry"
            onClick={onRetryQuota}
            disabled={quotaRefreshing}
          >
            <RefreshCw size={12} aria-hidden className={quotaRefreshing ? "hb-spin" : ""} />
            {quotaRefreshing ? "正在获取…" : "重试获取额度"}
          </button>
        ) : null}
      </section>

      {/* 4. 事件时间线 */}
      <section className="hb-radar-card">
        <h3 className="hb-radar-card-title">事件时间线</h3>
        {event && event.timeline.length > 0 ? (
          <div className="hb-timeline">
            {event.timeline.map((node) => (
              <div className="hb-timeline-node" key={`${node.kind}-${node.at}`}>
                <span className="hb-timeline-dot" data-kind={node.kind} aria-hidden />
                <span className="hb-timeline-label">{node.label}</span>
                <span className="hb-timeline-time">{formatHoverbarClock(node.at)}</span>
              </div>
            ))}
          </div>
        ) : (
          <p className="hb-radar-meta">暂无事件时间线。</p>
        )}
      </section>

      {/* 5. 当前事件相关动态 */}
      <section className="hb-radar-card">
        <h3 className="hb-radar-card-title">当前事件相关动态</h3>
        {eventPosts.length === 0 ? (
          <p className="hb-radar-meta">暂无关联动态。</p>
        ) : (
          eventPosts.map((post) => (
            <RadarPostItem
              key={post.id}
              post={post}
              translating={translate.isPending && translate.variables === post.id}
              onTranslate={() => translate.mutate(post.id)}
            />
          ))
        )}
        {translate.error ? (
          <p className="hb-radar-error">{ipcErrorMessage(translate.error, "翻译失败")}</p>
        ) : null}
      </section>

      {refreshError ? <p className="hb-radar-error">{refreshError}</p> : null}
      <p className="hb-radar-footnote">仅为推测，不代表官方结论；重置时间以官方实际执行为准。</p>
    </div>
  );
}

/** AI 辅助判断的两行文案：主行为状态或当前结论；关闭时绝不展示历史正文。 */
function aiAssessmentLines(ai: RadarSnapshot["aiAssessment"] | undefined): {
  primary: string;
  secondary: string | null;
} {
  if (!ai) return { primary: "未加载", secondary: null };
  if (!ai.enabled) {
    return {
      primary: "AI 分析已关闭",
      secondary: ai.history ? `最近一次成功分析 ${formatHoverbarClock(ai.history.createdAt)}` : "未运行过 AI 分析",
    };
  }
  switch (ai.state) {
    case "pending":
      return {
        primary: ai.current?.conclusion ?? "已有事件结论",
        secondary: "有新动态待分析 · 下次检查时更新",
      };
    case "failed":
      return {
        primary: ai.latestError ?? "本次分析失败",
        secondary: ai.history ? `历史分析 ${formatHoverbarClock(ai.history.createdAt)} 保留` : null,
      };
    case "current": {
      const bits = [
        ai.current?.model ? `模型 ${ai.current.model}` : null,
        ai.current ? formatHoverbarClock(ai.current.createdAt) : null,
      ].filter(Boolean);
      return {
        primary: ai.current?.conclusion ?? "已分析",
        secondary: bits.length > 0 ? `${bits.join(" · ")} 分析` : null,
      };
    }
    default:
      return { primary: "未分析", secondary: "开启 AI 后随「立即检查」运行" };
  }
}

function QuotaVerificationRow({ item }: { item: QuotaVerification }) {
  return (
    <div className="hb-quota-row" data-status={item.status}>
      <div className="hb-quota-row-head">
        <b>{item.accountName}</b>
        <span className="hb-quota-window">{item.windowLabel ?? "套餐窗口"}</span>
        <span className="hb-quota-status">{quotaBadgeLabel(item.status, item.attribution, item.lastResetObservedAt)}</span>
      </div>
      {item.status === "unavailable" ? (
        <p className="hb-radar-meta">网络无法获取额度，不影响来源与 AI 判断。</p>
      ) : null}
      {item.note ? <p className="hb-radar-meta">{item.note}</p> : null}
      <p className="hb-radar-meta">
        {item.lastSuccessAt ? `上次成功 ${formatHoverbarClock(item.lastSuccessAt)}` : "尚无成功快照"}
      </p>
      <p className="hb-radar-meta hb-quota-detail">
        {item.previous?.remaining != null && item.current?.remaining != null
          ? `剩余 ${Math.round(item.previous.remaining * 100)}% → ${Math.round(item.current.remaining * 100)}%`
          : null}
        {item.current?.resetAt
          ? `${item.previous?.remaining != null ? " · " : ""}重置 ${formatHoverbarClock(item.current.resetAt)}`
          : null}
        {quotaCorrelationLabel(item.temporalCorrelation)
          ? ` · ${quotaCorrelationLabel(item.temporalCorrelation)}`
          : null}
        {item.attribution === "user_confirmed" ? " · 用户已确认" : null}
      </p>
    </div>
  );
}

function RadarPostItem({
  post,
  translating,
  onTranslate,
}: {
  post: RadarPost;
  translating: boolean;
  onTranslate: () => void;
}) {
  const [linkError, setLinkError] = useState<string | null>(null);
  return (
    <article className="hb-radar-post">
      <div className="hb-radar-post-head">
        <span className="hb-radar-post-badge" data-signal={post.explicitReset ? "reset" : post.filter}>
          {POST_BADGE_LABEL[post.badge] ?? post.badge}
        </span>
        <span className="hb-radar-post-time">{formatHoverbarClock(post.postedAt)}</span>
      </div>
      <p className="hb-radar-post-text" data-selectable="true">
        {post.summary ?? post.translatedText ?? post.text}
      </p>
      <div className="hb-radar-post-actions">
        {post.translatedText ? (
          <span className="hb-radar-post-source" title={post.translatedAt ? formatHoverbarClock(post.translatedAt) : undefined}>
            {post.translationSource === "codexradar" ? "CodexRadar 中文" : `译自 ${post.translationSource ?? "未知来源"}`}
          </span>
        ) : (
          <button type="button" className="hb-radar-post-button" onClick={onTranslate} disabled={translating}>
            <Languages size={12} aria-hidden />
            {translating ? "翻译中…" : "翻译"}
          </button>
        )}
        {post.url ? (
          <button
            type="button"
            className="hb-radar-post-button"
            onClick={() => {
              setLinkError(null);
              void openExternalUrl(post.url).catch((error) => {
                setLinkError(ipcErrorMessage(error, "无法打开原文"));
              });
            }}
          >
            <ExternalLink size={12} aria-hidden />
            查看原文
          </button>
        ) : null}
      </div>
      {linkError ? <p className="hb-radar-error">{linkError}</p> : null}
    </article>
  );
}
