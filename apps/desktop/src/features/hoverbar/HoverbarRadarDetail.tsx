/**
 * 悬浮详情内的 GPT 重置雷达二级页。
 * 固定顺序：sticky 工具栏 → 重置判断 → 判断依据 → 本机验证 → Tibo 动态 → 最近一次事件。
 */
import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import {
  ArrowLeft,
  ChevronDown,
  ChevronRight,
  ExternalLink,
  Languages,
  RefreshCw,
} from "lucide-react";
import {
  confirmRadarUserReset,
  ipcErrorMessage,
  openExternalUrl,
  translateRadarPost,
  undoRadarUserReset,
  type QuotaVerification,
  type RadarAnalysis,
  type RadarPost,
  type RadarSnapshot,
} from "@/lib/ipc";
import { RADAR_SNAPSHOT_QUERY_KEY } from "@/lib/query-client";
import {
  formatHoverbarClock,
  formatRadarRangeLabel,
  humanizeRadarPostRefs,
  postsInRadarRange,
  quotaBadgeLabel,
  quotaCorrelationLabel,
  radarAiStatusLabel,
  radarCloseReasonLabel,
  radarConfirmationSourceLabel,
  radarDecisionBadge,
  radarDecisionTimeText,
  radarDeltaImpactLine,
  radarQuotaSummaryLine,
  sourceRelationLabel,
} from "./hoverbar-state";

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
  const confirmReset = useMutation({
    mutationFn: confirmRadarUserReset,
    onSuccess: (snapshot) => queryClient.setQueryData(RADAR_SNAPSHOT_QUERY_KEY, snapshot),
  });
  const undoReset = useMutation({
    mutationFn: undoRadarUserReset,
    onSuccess: (snapshot) => queryClient.setQueryData(RADAR_SNAPSHOT_QUERY_KEY, snapshot),
  });
  const [quotaDetailOpen, setQuotaDetailOpen] = useState(false);
  const [recentEventOpen, setRecentEventOpen] = useState(false);
  const [analysisOpen, setAnalysisOpen] = useState(false);
  const [historyAnalysisOpen, setHistoryAnalysisOpen] = useState(false);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [tiboLimit, setTiboLimit] = useState(12);

  const decision = radar?.decision ?? null;
  // “最近一次重置”只认本机观察/用户确认；普通关闭事件兜底为“最近一次事件”（来源声称）。
  const recentReset = decision?.recentReset ?? null;
  const recentClosedEvent = decision?.recentClosedEvent ?? null;
  const recentCard = recentReset ?? recentClosedEvent;
  const recentResetAt = recentReset?.observedResetAt ?? recentReset?.userConfirmedResetAt ?? null;
  const recentToggleMeta = recentResetAt
    ? `${formatHoverbarClock(recentResetAt)} · ${radarConfirmationSourceLabel(recentReset?.confirmationSource)}`
    : radarCloseReasonLabel(recentCard?.closeReason);
  const knownPosts = radar?.posts ?? [];
  const rangeKey = radar?.analysisPrefs.rangeKey;
  const rangePosts = postsInRadarRange(knownPosts, rangeKey).slice().sort((a, b) => b.postedAt - a.postedAt);
  const verifications = radar?.quotaVerifications ?? [];
  const quotaSummary = radarQuotaSummaryLine(verifications);
  // 当前分析选择规则：有活动事件优先 eventAnalysis，无活动事件只读 latestDeltaAnalysis。
  const aiAssessment = radar?.aiAssessment ?? null;
  const aiAnalysis = aiAssessment?.eventAnalysis ?? aiAssessment?.latestDeltaAnalysis ?? null;
  const aiStatusLabel = radarAiStatusLabel(aiAssessment);
  // AI 关闭/历史态：旧结果只能作为历史结果折叠查看，不冒充当前结论。
  const showCurrentAi = Boolean(aiAssessment?.enabled && aiAssessment.state !== "historical" && aiAnalysis);
  const historicalAnalysis = aiAssessment?.history ?? aiAnalysis;
  const rangeTitle =
    radar?.analysisPrefs.analyze
      ? "当前检查将使用主窗口中保存的分析范围"
      : "仅影响 Tibo 列表与启用 AI 后的背景范围；来源同步不受范围限制";
  const waitingVerify = decision
    ? ["expected_time_passed", "landed_claimed", "user_confirmed"].includes(decision.status)
    : false;
  const quotaUnavailable = verifications.some((item) => item.status === "unavailable");

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

      <section className="hb-radar-card">
        <div className="hb-radar-card-head">
          {decision ? (
            <span className="radar-phase-badge ml-auto" data-phase={decision.status}>
              {radarDecisionBadge(decision)}
            </span>
          ) : null}
        </div>
        {decision ? (
          <>
            <p className="hb-radar-headline" data-selectable="true">
              {decision.headline}
            </p>
            <p className="hb-radar-copy" data-time-kind={decision.timeKind}>
              {radarDecisionTimeText(decision)}
            </p>
            {decision.verificationHint ? <p className="hb-radar-copy">{decision.verificationHint}</p> : null}
            {decision.observationPeriodText ? (
              <p className="hb-radar-meta" data-observation="true">
                {decision.observationPeriodText}
              </p>
            ) : null}
            {radar ? <p className="hb-radar-copy">{radarDeltaImpactLine(radar)}</p> : null}
            {decision.status === "no_signal" && decision.recentSummaryText ? (
              <p className="hb-radar-meta">{decision.recentSummaryText}</p>
            ) : null}
            {decision.canConfirmReset ? (
              <button type="button" className="hb-radar-post-button" onClick={() => setConfirmOpen(true)}>
                确认额度已重置
              </button>
            ) : null}
            {decision.canUndoConfirm ? (
              <button
                type="button"
                className="hb-radar-post-button"
                disabled={undoReset.isPending}
                onClick={() => undoReset.mutate()}
              >
                撤销人工确认
              </button>
            ) : null}
          </>
        ) : (
          <p className="hb-radar-meta">正在加载重置判断…</p>
        )}
      </section>

      {/* CodexRadar 公告：无公告时压缩为单行，不换行 */}
      {radar?.notice ? (
        <section className="hb-radar-card hb-notice-card">
          <h3 className="hb-radar-card-title">
            {radar.notice.isCurrent ? "CodexRadar 公告" : "CodexRadar 最近公告"}
          </h3>
          <p className="hb-radar-subheadline" data-selectable="true">
            {radar.notice.headline}
          </p>
          {radar.notice.lead ? (
            <p className="hb-radar-meta" data-selectable="true">
              {radar.notice.lead}
            </p>
          ) : null}
          <p className="hb-radar-meta">
            {radar.notice.isCurrent
              ? radar.notice.updatedAt
                ? `更新 ${formatHoverbarClock(radar.notice.updatedAt)}`
                : "当前公告"
              : radar.notice.updatedAt
                ? `上次出现于 ${formatHoverbarClock(radar.notice.updatedAt)}`
                : "历史公告"}
          </p>
        </section>
      ) : (
        <section className="hb-radar-card hb-notice-card hb-notice-empty">
          <h3 className="hb-radar-card-title hb-nowrap">CodexRadar 当前无公告</h3>
          <span className="hb-radar-meta hb-nowrap">帖子同步正常</span>
        </section>
      )}

      {/* AI 分析：结论/分析同级标签；AI 关闭或历史态时旧结果折叠查看 */}
      <section className="hb-radar-card hb-ai-card">
        <div className="hb-radar-card-head">
          <h3 className="hb-radar-card-title">AI 分析</h3>
          <span className="radar-phase-badge" data-phase={aiAssessment?.state === "covered" ? "landed_observed" : aiAssessment?.state === "failed" ? "closed" : "upcoming"}>
            {aiStatusLabel}
          </span>
        </div>
        {showCurrentAi && aiAnalysis ? (
          <AiReasoningBlock
            analysis={aiAnalysis}
            posts={knownPosts}
            rangeKey={rangeKey ?? null}
            customPrompt={Boolean(
              radar?.analysisPrefs.userPrompt.trim() &&
                radar.analysisPrefs.userPrompt !== radar.analysisPrefs.defaultUserPrompt,
            )}
            open={analysisOpen}
            onToggle={() => setAnalysisOpen((value) => !value)}
            citationLimit={2}
          />
        ) : (
          <>
            <p className="hb-radar-meta">
              {!aiAssessment || !aiAssessment.enabled || aiAssessment.state === "disabled"
                ? "AI 未启用：来源公告与本机验证不受影响。"
                : "没有针对当前范围的新分析；以下为最近一次历史结果。有新增动态时，下次检查会重新分析。"}
            </p>
            {historicalAnalysis?.conclusion ? (
              <>
                <button
                  type="button"
                  className="hb-ai-detail-toggle"
                  data-open={historyAnalysisOpen || undefined}
                  onClick={() => setHistoryAnalysisOpen((value) => !value)}
                >
                  {historyAnalysisOpen ? "收起历史分析" : "查看历史分析"}
                  <ChevronDown size={12} aria-hidden />
                </button>
                <div className="hb-radar-collapse" data-open={historyAnalysisOpen || undefined} aria-hidden={!historyAnalysisOpen}>
                  <div className="hb-radar-collapse-inner hb-radar-recent-body">
                    <p className="hb-radar-field-label">历史结论</p>
                    <p className="hb-radar-subheadline" data-selectable="true">
                      {humanizeRadarPostRefs(historicalAnalysis.conclusion, knownPosts)}
                    </p>
                    {historicalAnalysis.analysisBasis ? (
                      <>
                        <p className="hb-radar-field-label">历史分析</p>
                        <p className="hb-radar-copy" data-selectable="true">
                          {humanizeRadarPostRefs(historicalAnalysis.analysisBasis, knownPosts)}
                        </p>
                      </>
                    ) : null}
                    <p className="hb-radar-meta">
                      {historicalAnalysis.model ?? "未知模型"}
                      {` · ${formatHoverbarClock(historicalAnalysis.createdAt)}`}
                      {historicalAnalysis.rangeKey ? ` · ${formatRadarRangeLabel(historicalAnalysis.rangeKey, true)}` : ""}
                    </p>
                  </div>
                </div>
              </>
            ) : null}
          </>
        )}
        {radar?.aiAssessment.latestError ? (
          <p className="hb-radar-error">{radar.aiAssessment.latestError}</p>
        ) : null}
      </section>

      <section className="hb-radar-card">
        <p className="hb-radar-subheadline" data-selectable="true">
          {waitingVerify
            ? "本机尚未观察到额度重置"
            : quotaUnavailable
              ? "本机暂无法验证"
              : quotaSummary}
        </p>
        {waitingVerify ? <p className="hb-radar-meta">等待本机检测或用户确认</p> : null}
        {quotaUnavailable ? <p className="hb-radar-meta">不影响来源与 AI 判断</p> : null}
        <div className="hb-radar-collapse" data-open={quotaDetailOpen || undefined} aria-hidden={!quotaDetailOpen}>
          <div className="hb-radar-collapse-inner">
            {verifications.length === 0 ? (
              <p className="hb-radar-meta">未接入 GPT 额度来源。</p>
            ) : (
              verifications.map((item) => <QuotaVerificationRow key={item.sourceId} item={item} />)
            )}
          </div>
        </div>
        <div className="hb-radar-post-actions hb-quota-summary-actions">
          {onRetryQuota && quotaUnavailable ? (
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
          {verifications.length > 0 ? (
            <button
              type="button"
              className="hb-ai-detail-toggle"
              onClick={() => setQuotaDetailOpen((open) => !open)}
            >
              {quotaDetailOpen ? "收起账号详情" : "查看账号详情"}
              <ChevronDown size={12} aria-hidden className={quotaDetailOpen ? "hb-rotate-180" : ""} />
            </button>
          ) : null}
        </div>
      </section>

      <section className="hb-radar-card">
        <div className="hb-radar-card-head">
          <h3 className="hb-radar-card-title">Tibo 动态</h3>
          <span className="hb-radar-card-meta">{formatRadarRangeLabel(rangeKey)} · {rangePosts.length} 条</span>
        </div>
        {rangePosts.length === 0 ? (
          <p className="hb-radar-meta">当前范围内没有动态。</p>
        ) : (
          rangePosts.slice(0, tiboLimit).map((post) => (
            <RadarPostItem
              key={post.id}
              post={post}
              translating={translate.isPending && translate.variables === post.id}
              onTranslate={() => translate.mutate(post.id)}
            />
          ))
        )}
        {rangePosts.length > tiboLimit ? (
          <button type="button" className="hb-radar-post-button" onClick={() => setTiboLimit(rangePosts.length)}>
            查看全部 {rangePosts.length} 条
          </button>
        ) : null}
        {translate.error ? (
          <p className="hb-radar-error">{ipcErrorMessage(translate.error, "翻译失败")}</p>
        ) : null}
      </section>

      {recentCard ? (
        <section className="hb-radar-card hb-radar-card-compact">
          <button
            type="button"
            className="hb-radar-card-toggle"
            onClick={() => setRecentEventOpen((open) => !open)}
          >
            <ChevronRight
              size={13}
              aria-hidden
              className={recentEventOpen ? "hb-rotate-90" : ""}
            />
            <span className="hb-radar-card-toggle-title">
              {recentReset ? "最近一次重置" : "最近一次事件"}
            </span>
            <span className="hb-radar-card-toggle-meta">
              {recentToggleMeta}
            </span>
          </button>
          <div className="hb-radar-collapse" data-open={recentEventOpen || undefined} aria-hidden={!recentEventOpen}>
            <div className="hb-radar-collapse-inner hb-radar-recent-body">
              <p className="hb-radar-meta">
                {radarConfirmationSourceLabel(recentCard.confirmationSource)} ·{" "}
                {radarCloseReasonLabel(recentCard.closeReason)}
              </p>
              {recentCard.analysis?.conclusion ? (
                <>
                  <p className="hb-radar-field-label">当时结论</p>
                  <p className="hb-radar-subheadline" data-selectable="true">
                    {humanizeRadarPostRefs(recentCard.analysis.conclusion, knownPosts)}
                  </p>
                </>
              ) : null}
              {recentCard.analysis?.analysisBasis ? (
                <>
                  <p className="hb-radar-field-label">当时分析</p>
                  <p className="hb-radar-copy" data-selectable="true">
                    {humanizeRadarPostRefs(recentCard.analysis.analysisBasis, knownPosts)}
                  </p>
                </>
              ) : null}
            </div>
          </div>
        </section>
      ) : null}

      {refreshError ? <p className="hb-radar-error">{refreshError}</p> : null}
      {confirmReset.error ? (
        <p className="hb-radar-error">{ipcErrorMessage(confirmReset.error, "确认失败")}</p>
      ) : null}
      {undoReset.error ? (
        <p className="hb-radar-error">{ipcErrorMessage(undoReset.error, "撤销失败")}</p>
      ) : null}
      <p className="hb-radar-footnote">仅为推测，不代表官方结论；重置时间以官方实际执行为准。</p>

      {confirmOpen ? (
        <ConfirmResetDialog
          pending={confirmReset.isPending}
          onCancel={() => setConfirmOpen(false)}
          onConfirm={() => {
            confirmReset.mutate(undefined, {
              onSuccess: () => setConfirmOpen(false),
            });
          }}
        />
      ) : null}
    </div>
  );
}

function ConfirmResetDialog({
  pending,
  onCancel,
  onConfirm,
}: {
  pending: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  return (
    <div
      className="hb-confirm-mask"
      role="presentation"
      onMouseDown={() => {
        if (!pending) onCancel();
      }}
    >
      <div
        className="hb-confirm-dialog"
        role="dialog"
        aria-modal="true"
        aria-label="确认额度已经重置"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <p className="hb-radar-subheadline">确认额度已经重置？</p>
        <p className="hb-radar-meta">
          这只记录你的人工观察，不代表官方确认，也不会判断是官方重置还是使用了重置卡。
        </p>
        <div className="hb-radar-post-actions">
          <button type="button" className="hb-radar-post-button" disabled={pending} onClick={onCancel}>
            取消
          </button>
          <button type="button" className="hb-radar-post-button" disabled={pending} onClick={onConfirm}>
            {pending ? "确认中…" : "确认"}
          </button>
        </div>
      </div>
    </div>
  );
}

function AiReasoningBlock({
  analysis,
  posts,
  rangeKey,
  customPrompt,
  open,
  onToggle,
  citationLimit,
}: {
  analysis: RadarAnalysis;
  posts: RadarPost[];
  rangeKey: string | null;
  customPrompt: boolean;
  open: boolean;
  onToggle: () => void;
  citationLimit: number;
}) {
  const byId = new Map(posts.map((post) => [post.id, post]));
  // 正向依据引用只取 citations ∩ newPostIds；历史上下文引用不进入当前依据。
  const currentCitationPosts = analysis.citations
    .filter((id) => analysis.newPostIds.includes(id))
    .map((id) => byId.get(id))
    .filter((post): post is RadarPost => Boolean(post));
  const visible = currentCitationPosts.slice(0, citationLimit);
  const irrelevant = analysis.eventRelation === "none";
  return (
    <div className="hb-ai-block">
      {analysis.conclusion ? (
        <>
          <p className="hb-radar-field-label">结论</p>
          <p className="hb-radar-subheadline" data-selectable="true">
            {humanizeRadarPostRefs(analysis.conclusion, posts)}
          </p>
        </>
      ) : null}
      {analysis.analysisBasis ? (
        <>
          <p className="hb-radar-field-label">分析</p>
          <p className="hb-radar-copy" data-selectable="true">
            {humanizeRadarPostRefs(analysis.analysisBasis, posts)}
          </p>
        </>
      ) : null}
      <p className="hb-radar-meta">
        {analysis.model ?? "未知模型"}
        {` · ${formatHoverbarClock(analysis.createdAt)}`}
        {rangeKey ? ` · ${formatRadarRangeLabel(rangeKey, true)}` : ""}
        {customPrompt ? " · 自定义语义提示" : ""}
      </p>
      {visible.length > 0 ? (
        visible.map((post) => (
          <CitationChip key={post.id} post={post} tag={irrelevant ? "无关信号" : "直接信号"} />
        ))
      ) : (
        <p className="hb-radar-meta">本轮没有引用重置相关帖子。</p>
      )}
      {currentCitationPosts.length > citationLimit ? (
        <p className="hb-radar-meta">查看全部 {currentCitationPosts.length} 条</p>
      ) : null}
      <button type="button" className="hb-ai-detail-toggle" data-open={open || undefined} onClick={onToggle}>
        {open ? "收起分析详情" : "查看分析详情"}
        <ChevronDown size={12} aria-hidden />
      </button>
      <div className="hb-radar-collapse" data-open={open || undefined} aria-hidden={!open}>
        <div className="hb-radar-collapse-inner hb-ai-detail-body">
          {analysis.support.map((item, index) => (
            <p className="hb-radar-meta" data-selectable="true" key={`support-${index}`}>
              支持 · {humanizeRadarPostRefs(item, posts)}
            </p>
          ))}
          {analysis.against.map((item, index) => (
            <p className="hb-radar-meta" data-selectable="true" key={`against-${index}`}>
              反向 · {humanizeRadarPostRefs(item, posts)}
            </p>
          ))}
          {analysis.uncertainty.map((item, index) => (
            <p className="hb-radar-meta" data-selectable="true" key={`uncertain-${index}`}>
              不确定 · {humanizeRadarPostRefs(item, posts)}
            </p>
          ))}
        </div>
      </div>
    </div>
  );
}

function CitationChip({ post, tag }: { post: RadarPost; tag?: string }) {
  const [linkError, setLinkError] = useState<string | null>(null);
  return (
    <span className="hb-citation-chip">
      {tag ? <span className={irrelevantTagClass(tag)}>{tag}</span> : null}
      <span className="hb-evidence-time">{formatHoverbarClock(post.postedAt)}</span>
      <button
        type="button"
        className="hb-radar-post-button"
        onClick={() => {
          setLinkError(null);
          void openExternalUrl(post.url).catch((error) => {
            setLinkError(ipcErrorMessage(error, "无法打开原帖"));
          });
        }}
      >
        <ExternalLink size={11} aria-hidden />
        查看原帖
      </button>
      {linkError ? <p className="hb-radar-error">{linkError}</p> : null}
    </span>
  );
}

/** 引用标签配色：无关信号中性灰、直接信号暖色，其余蓝色。 */
function irrelevantTagClass(tag: string): string {
  if (tag === "无关信号") return "hb-signal-tag hb-signal-tag-none";
  if (tag === "直接信号") return "hb-signal-tag hb-signal-tag-direct";
  return "hb-signal-tag hb-signal-tag-related";
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
      {item.note ? <p className="hb-radar-meta hb-radar-meta-strong">{item.note}</p> : null}
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
        {item.attribution === "user_confirmed" ? " · 用户已确认重置卡" : null}
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
        <span className="hb-source-tag" title="来源分类" data-signal={post.explicitReset ? "reset" : post.filter}>
          {sourceRelationLabel(post)}
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
            查看原帖
          </button>
        ) : null}
      </div>
      {linkError ? <p className="hb-radar-error">{linkError}</p> : null}
    </article>
  );
}
