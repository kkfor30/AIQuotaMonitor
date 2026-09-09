/**
 * 悬浮详情内的 GPT 重置雷达二级页。
 * 布局：sticky 工具栏 → 核心判断卡（结论+时间+确认操作）
 * → AI 分析卡（结论+依据+原帖引用直接可见）
 * → 本机验证卡（默认折叠，双列额度变化与重置卡按需展开）
 * → 最新动态卡（精选动态直接可见）
 * → CodexRadar 公告（按需）→ 最近一次历史重置（按需折叠）。
 */
import { useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import {
  ArrowLeft,
  ChevronDown,
  ChevronRight,
  CreditCard,
  ExternalLink,
  Languages,
  Maximize2,
  RefreshCw,
} from "lucide-react";
import {
  confirmRadarUserReset,
  ipcErrorMessage,
  openExternalUrl,
  openRadarPage,
  setRadarNoticeHidden,
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
  quotaBadgeLabelCompact,
  quotaCorrelationLabel,
  radarAccountBankedChangeNotes,
  radarAiStatusLabel,
  radarBankedChangeLine,
  radarBankedChangeRows,
  radarCloseReasonLabel,
  radarConfirmationSourceLabel,
  radarConfirmResetDialogBody,
  radarConfirmResetDialogTitle,
  radarConfirmResetLabel,
  radarDecisionBadge,
  radarDecisionTimeText,
  radarDeltaImpactLine,
  radarQuotaSummaryLine,
  radarSignalTypeLabel,
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
    mutationFn: () =>
      confirmRadarUserReset({
        eventId: radar?.decision.activeEventId ?? null,
        expectedRevision:
          radar?.activeEvents?.find((event) => event.id === radar.decision.activeEventId)?.stateRevision ??
          radar?.event?.stateRevision ??
          null,
      }),
    onSuccess: (snapshot) => queryClient.setQueryData(RADAR_SNAPSHOT_QUERY_KEY, snapshot),
  });
  const undoReset = useMutation({
    mutationFn: () =>
      undoRadarUserReset({
        eventId: radar?.decision.activeEventId ?? null,
        expectedRevision:
          radar?.activeEvents?.find((event) => event.id === radar.decision.activeEventId)?.stateRevision ??
          radar?.event?.stateRevision ??
          null,
      }),
    onSuccess: (snapshot) => queryClient.setQueryData(RADAR_SNAPSHOT_QUERY_KEY, snapshot),
  });
  const setNoticeHidden = useMutation({
    mutationFn: setRadarNoticeHidden,
    onSuccess: (snapshot) => queryClient.setQueryData(RADAR_SNAPSHOT_QUERY_KEY, snapshot),
  });

  const pageRef = useRef<HTMLDivElement>(null);
  const [aiDetailOpen, setAiDetailOpen] = useState(false);
  const [verificationOpen, setVerificationOpen] = useState(false);
  const [recentEventOpen, setRecentEventOpen] = useState(false);
  const [tiboLimit, setTiboLimit] = useState(4);
  const [confirmOpen, setConfirmOpen] = useState(false);

  const decision = radar?.decision ?? null;
  const recentReset = decision?.recentReset ?? null;
  const recentClosedEvent = decision?.recentClosedEvent ?? null;
  const recentCard = recentReset ?? recentClosedEvent;
  const recentResetAt = recentReset?.observedResetAt ?? recentReset?.userConfirmedResetAt ?? null;
  const recentToggleMeta = recentResetAt
    ? `${formatHoverbarClock(recentResetAt)} · ${radarConfirmationSourceLabel(recentReset?.confirmationSource)}`
    : radarCloseReasonLabel(recentCard?.closeReason);

  const knownPosts = radar?.posts ?? [];
  const verifications = radar?.quotaVerifications ?? [];
  const quotaSummary = radarQuotaSummaryLine(verifications);

  const aiAssessment = radar?.aiAssessment ?? null;
  const aiAnalysis = aiAssessment?.eventAnalysis ?? aiAssessment?.latestDeltaAnalysis ?? null;
  const aiStatusLabel = radarAiStatusLabel(aiAssessment);
  const showCurrentAi = Boolean(aiAssessment?.enabled && aiAnalysis);

  const quotaUnavailable = verifications.length > 0 && verifications.every((item) => item.status === "unavailable");

  // 整理本机验证副标题：精准表达当前状态，用户已确认或已观察时不再显示“等待确认”
  const verificationSubtitle = (() => {
    if (verifications.length === 0) return "未接入 GPT 额度来源";
    if (quotaUnavailable) return "本机暂无法验证";
    const observed = verifications.find((item) => item.lastResetObservedAt);
    if (observed?.lastResetObservedAt) {
      return `本机于 ${formatHoverbarClock(observed.lastResetObservedAt)} 观察到刷新`;
    }
    if (decision?.status === "user_confirmed") {
      return "用户已确认 · 本机未见额度变化";
    }
    if (decision && ["expected_time_passed", "landed_claimed"].includes(decision.status)) {
      return "等待本机检测或用户确认";
    }
    return quotaSummary;
  })();

  // 整理决策时间展示：避免 headline 与 timeText 重复叙述
  let decisionTimeDisplay: string | null = decision?.timeText ? radarDecisionTimeText(decision) : null;
  if (decisionTimeDisplay && decision?.headline) {
    if (decision.status === "user_confirmed") {
      decisionTimeDisplay = decisionTimeDisplay.replace(/确认额度已重置|确认重置卡已到账/, "确认").trim();
    } else if (decisionTimeDisplay === decision.headline) {
      decisionTimeDisplay = null;
    }
  }

  const confirmDialog = confirmOpen ? (
    <ConfirmResetDialog
      eventType={decision?.eventType}
      pending={confirmReset.isPending}
      onCancel={() => setConfirmOpen(false)}
      onConfirm={() => {
        confirmReset.mutate(undefined, {
          onSuccess: () => setConfirmOpen(false),
        });
      }}
    />
  ) : null;
  const confirmHost = pageRef.current?.closest(".hb-panel") ?? pageRef.current;

  return (
    <div ref={pageRef} className="hb-radar-page">
      {/* 顶部工具栏：左右两端对齐，防溢出图标按钮 */}
      <div className="hb-radar-top">
        <button type="button" className="hb-radar-back" onClick={onBack}>
          <ArrowLeft size={13} aria-hidden />
          返回额度
        </button>
        <div className="hb-radar-top-actions">
          {onRefresh ? (
            <button
              type="button"
              className="hb-radar-refresh"
              onClick={refreshing && onCancel ? onCancel : onRefresh}
              data-loading={refreshing || undefined}
              aria-label={refreshing ? "终止检查" : "刷新重置信号"}
              title={refreshing ? "终止检查" : "刷新重置信号"}
            >
              <RefreshCw size={12} aria-hidden className={refreshing ? "hb-spin" : ""} />
              {refreshing ? "终止" : "刷新"}
            </button>
          ) : null}
          <span className="hb-radar-pill">仅为推测</span>
          <button
            type="button"
            className="hb-radar-icon-btn"
            title="在主窗口打开完整雷达"
            aria-label="在主窗口打开完整雷达"
            onClick={() => {
              void openRadarPage().catch(() => {});
            }}
          >
            <Maximize2 size={13} aria-hidden />
          </button>
        </div>
      </div>

      {/* 1. 核心判断卡：结论、时间、观察期与确认操作直接可用 */}
      <section className="hb-radar-card hb-radar-hero-card" data-status={decision?.status}>
        <div className="hb-radar-card-head">
          <span className="hb-radar-hero-tag">核心判断</span>
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
            {decisionTimeDisplay ? (
              <p className="hb-radar-copy" data-time-kind={decision.timeKind}>
                {decisionTimeDisplay}
              </p>
            ) : null}
            {decision.verificationHint ? <p className="hb-radar-copy">{decision.verificationHint}</p> : null}
            {decision.observationPeriodText ? (
              <p className="hb-radar-meta" data-observation="true">
                {decision.observationPeriodText}
              </p>
            ) : null}
            {radar && radarDeltaImpactLine(radar) ? (
              <p className="hb-radar-copy">{radarDeltaImpactLine(radar)}</p>
            ) : null}
            {radarBankedChangeRows(decision ?? { recentBankedGrant: null, recentBankedDecrease: null }).map((change) => (
              <p
                key={`${change.kind}-${change.observedAt}-${change.sourceId}`}
                className="hb-radar-copy flex items-center gap-1.5"
                data-selectable="true"
              >
                <CreditCard size={12} className="shrink-0 text-q-primary/80" aria-hidden />
                <span>{radarBankedChangeLine(change, formatHoverbarClock)}</span>
              </p>
            ))}
            {!recentCard && decision.status === "no_signal" && decision.recentSummaryText ? (
              <p className="hb-radar-meta">{decision.recentSummaryText}</p>
            ) : null}
            {decision.canConfirmReset ? (
              <button type="button" className="hb-radar-post-button" onClick={() => setConfirmOpen(true)}>
                {radarConfirmResetLabel(decision.eventType)}
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

      {/* 2. AI 分析卡：结论与依据直接呈现，详细支持/反对项支持展开 */}
      {aiAssessment?.enabled ? (
        <section className="hb-radar-card">
          <div className="hb-radar-card-head">
            <h3 className="hb-radar-card-title">AI 分析</h3>
            <span
              className="radar-phase-badge"
              data-phase={
                aiAssessment?.state === "covered"
                  ? "landed_observed"
                  : aiAssessment?.state === "failed"
                    ? "closed"
                    : "upcoming"
              }
            >
              {aiStatusLabel}
            </span>
          </div>
          {showCurrentAi && aiAnalysis ? (
            <AiReasoningBlock
              analysis={aiAnalysis}
              posts={knownPosts}
              rangeKey={radar?.analysisPrefs ? "monitor:72h" : null}
              customPrompt={Boolean(
                radar?.analysisPrefs.userPrompt.trim() &&
                  radar.analysisPrefs.userPrompt !== radar.analysisPrefs.defaultUserPrompt,
              )}
              open={aiDetailOpen}
              onToggle={() => setAiDetailOpen((open) => !open)}
            />
          ) : (
            <p className="hb-radar-meta">
              {aiAssessment.state === "failed"
                ? (radar?.aiAssessment.latestError ? `AI 分析失败：${radar.aiAssessment.latestError}` : "AI 分析失败。")
                : "当前时间窗内没有 Tibo 新动态；有新增动态时将自动分析。"}
            </p>
          )}
        </section>
      ) : null}

      {/* 3. 本机额度验证：默认折叠成单行，账号/窗口细节按需展开；重试动作保持常驻 */}
      <section className="hb-radar-card hb-radar-card-compact">
        <button
          type="button"
          className="hb-radar-card-toggle"
          onClick={() => setVerificationOpen((open) => !open)}
        >
          <ChevronRight
            size={13}
            aria-hidden
            className={verificationOpen ? "hb-rotate-90" : ""}
          />
          <span className="hb-radar-card-toggle-title">本机验证</span>
          <span className="hb-radar-card-toggle-meta">{verificationSubtitle}</span>
        </button>
        <div className="hb-radar-collapse" data-open={verificationOpen || undefined} aria-hidden={!verificationOpen}>
          <div className="hb-radar-collapse-inner hb-verification-body">
            {verifications.length === 0 ? (
              <p className="hb-radar-meta">未接入 GPT 额度来源。</p>
            ) : (
              verifications.map((item) => <QuotaVerificationRow key={item.sourceId} item={item} />)
            )}
          </div>
        </div>
        {onRetryQuota && quotaUnavailable ? (
          <div className="hb-radar-post-actions hb-quota-summary-actions">
            <button
              type="button"
              className="hb-quota-retry"
              onClick={onRetryQuota}
              disabled={quotaRefreshing}
            >
              <RefreshCw size={12} aria-hidden className={quotaRefreshing ? "hb-spin" : ""} />
              {quotaRefreshing ? "正在获取…" : "重试获取额度"}
            </button>
          </div>
        ) : null}
      </section>

      {/* 4. 最新动态卡：首屏直出最新几条动态，免去折叠展开 */}
      <section className="hb-radar-card">
        <div className="hb-radar-card-head">
          <h3 className="hb-radar-card-title">最新动态</h3>
          <span className="hb-radar-meta">{knownPosts.length} 条已同步</span>
        </div>
        {knownPosts.length === 0 ? (
          <p className="hb-radar-meta">尚未同步动态。</p>
        ) : (
          knownPosts.slice(0, tiboLimit).map((post) => (
            <RadarPostItem
              key={post.id}
              post={post}
              translating={translate.isPending && translate.variables === post.id}
              onTranslate={() => translate.mutate(post.id)}
            />
          ))
        )}
        {knownPosts.length > tiboLimit ? (
          <button
            type="button"
            className="hb-radar-post-button"
            onClick={() => setTiboLimit((prev) => prev + 6)}
          >
            查看更多动态（还有 {knownPosts.length - tiboLimit} 条）
          </button>
        ) : null}
        <div className="hb-radar-meta mt-1">
          <button
            type="button"
            className="hb-ai-detail-toggle"
            onClick={() => {
              void openRadarPage().catch(() => {});
            }}
          >
            在主窗口查看全部动态与时间线 →
          </button>
        </div>
        {translate.error ? (
          <p className="hb-radar-error">{ipcErrorMessage(translate.error, "翻译失败")}</p>
        ) : null}
      </section>

      {/* 5. CodexRadar 公告卡：按需展示 */}
      {!radar?.noticeHidden && radar?.notice ? (
        <section className="hb-radar-card hb-notice-card">
          <div className="hb-radar-card-head">
            <h3 className="hb-radar-card-title">
              {radar.notice.isCurrent ? "CodexRadar 公告" : "CodexRadar 最近公告"}
            </h3>
            <button
              type="button"
              className="hb-radar-post-button"
              disabled={setNoticeHidden.isPending}
              aria-label="隐藏 CodexRadar 公告"
              onClick={() => setNoticeHidden.mutate(true)}
            >
              隐藏
            </button>
          </div>
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
      ) : null}

      {/* 6. 最近一次重置/事件：低侵入紧凑折叠 */}
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
                  <p className="hb-radar-basis" data-selectable="true">
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

      {confirmDialog && confirmHost ? createPortal(confirmDialog, confirmHost) : confirmDialog}
    </div>
  );
}

function ConfirmResetDialog({
  eventType,
  pending,
  onCancel,
  onConfirm,
}: {
  eventType: string | null | undefined;
  pending: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const title = radarConfirmResetDialogTitle(eventType);
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
        aria-label={title}
        onMouseDown={(event) => event.stopPropagation()}
      >
        <p className="hb-radar-subheadline">{title}</p>
        <p className="hb-radar-meta">{radarConfirmResetDialogBody(eventType)}</p>
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

/** AI 解释正文：结论/分析/元信息/引用 chip 直接可见；支持反向展开。 */
function AiReasoningBlock({
  analysis,
  posts,
  rangeKey,
  customPrompt,
  open,
  onToggle,
}: {
  analysis: RadarAnalysis;
  posts: RadarPost[];
  rangeKey: string | null;
  customPrompt: boolean;
  open: boolean;
  onToggle: () => void;
}) {
  const byId = new Map(posts.map((post) => [post.id, post]));
  const currentCitationPosts = analysis.citations
    .filter((id) => analysis.newPostIds.includes(id))
    .map((id) => byId.get(id))
    .filter((post): post is RadarPost => Boolean(post));
  const visible = currentCitationPosts.slice(0, 2);
  const irrelevant = analysis.eventRelation === "none";

  return (
    <div className="hb-ai-block">
      {analysis.conclusion ? (
        <>
          <p className="hb-radar-field-label">结论</p>
          <p className="hb-radar-subheadline" data-selectable="true">
            {humanizeRadarPostRefs(analysis.conclusion, posts)}
          </p>
          {analysis.signalType ? (
            <p className="hb-radar-meta">信号类型 · {radarSignalTypeLabel(analysis.signalType)}</p>
          ) : null}
        </>
      ) : null}
      {analysis.analysisBasis ? (
        <>
          <p className="hb-radar-field-label">分析</p>
          <p className="hb-radar-basis" data-selectable="true">
            {humanizeRadarPostRefs(analysis.analysisBasis, posts)}
          </p>
        </>
      ) : null}
      <p className="hb-radar-meta">
        {analysis.model ?? "未知模型"}
        {` · ${formatHoverbarClock(analysis.createdAt)}`}
        {analysis.rangeKey ? ` · ${formatRadarRangeLabel(analysis.rangeKey, true)}` : rangeKey ? ` · ${formatRadarRangeLabel(rangeKey, true)}` : ""}
        {customPrompt ? " · 自定义语义提示" : ""}
      </p>
      {visible.length > 0 ? (
        visible.map((post) => (
          <CitationChip key={post.id} post={post} tag={irrelevant ? "无关信号" : "直接信号"} />
        ))
      ) : (
        <p className="hb-radar-meta">本轮没有引用重置相关帖子。</p>
      )}
      {analysis.support.length > 0 || analysis.against.length > 0 || analysis.uncertainty.length > 0 ? (
        <>
          <button
            type="button"
            className="hb-ai-detail-toggle"
            data-open={open || undefined}
            onClick={onToggle}
          >
            {open ? "收起分析详情" : "查看分析详情"}
            <ChevronDown size={12} aria-hidden className={open ? "hb-rotate-180" : ""} />
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
        </>
      ) : null}
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

function irrelevantTagClass(tag: string): string {
  if (tag === "无关信号") return "hb-signal-tag hb-signal-tag-none";
  if (tag === "直接信号") return "hb-signal-tag hb-signal-tag-direct";
  return "hb-signal-tag hb-signal-tag-related";
}

function formatRemainingPercent(val: number | null | undefined): string {
  if (val == null) return "0%";
  const normalized = val <= 1.0 ? val * 100 : val;
  return `${Math.round(normalized)}%`;
}

function QuotaVerificationRow({ item }: { item: QuotaVerification }) {
  const hasRemaining = item.previous?.remaining != null && item.current?.remaining != null;
  const bankedNotes = radarAccountBankedChangeNotes(item, formatHoverbarClock);

  return (
    <div className="hb-quota-row" data-status={item.status}>
      <div className="hb-quota-row-head">
        <b>{item.accountName}</b>
        <span className="hb-quota-window">{item.windowLabel ?? "套餐窗口"}</span>
        <span className="hb-quota-status">
          {quotaBadgeLabelCompact(item.status, item.attribution, item.lastResetObservedAt)}
        </span>
      </div>
      {item.status === "unavailable" ? (
        <p className="hb-radar-meta">网络无法获取额度，不影响来源与 AI 判断。</p>
      ) : null}
      {item.note ? <p className="hb-radar-meta hb-radar-meta-strong">{item.note}</p> : null}
      <div className="hb-quota-metrics-grid">
        <div className="hb-quota-metric-col">
          <span className="hb-quota-metric-label">额度变化</span>
          <span className="hb-quota-metric-val">
            {hasRemaining ? (
              <span className="tabular-nums font-semibold">
                {formatRemainingPercent(item.previous?.remaining)} → {formatRemainingPercent(item.current?.remaining)}
              </span>
            ) : (
              <span className="text-q-text-muted">未见变化</span>
            )}
          </span>
          <span className="hb-quota-metric-sub">
            {item.current?.resetAt ? `重置 ${formatHoverbarClock(item.current.resetAt)}` : ""}
            {item.lastSuccessAt ? ` · 快照 ${formatHoverbarClock(item.lastSuccessAt)}` : ""}
            {quotaCorrelationLabel(item.temporalCorrelation) ? ` · ${quotaCorrelationLabel(item.temporalCorrelation)}` : ""}
          </span>
        </div>
        <div className="hb-quota-metric-col">
          <span className="hb-quota-metric-label">重置卡与权益</span>
          <span className="hb-quota-metric-val">
            <span className="tabular-nums font-semibold">
              {item.bankedResetLabel}
            </span>
          </span>
          <span className="hb-quota-metric-sub" title={bankedNotes || undefined}>
            {bankedNotes || (item.attribution === "user_confirmed" ? "用户已确认重置卡" : "未见变动")}
          </span>
        </div>
      </div>
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
