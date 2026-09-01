/**
 * 悬浮详情内的 GPT 重置雷达二级页（信息架构 V2）。
 * 固定顺序：重置判断 → 判断依据（事实证据 / AI 推理依据）→ 本机验证摘要 → 时间线 → 相关动态 → 最近一次事件。
 * 判断结论由 Rust decision 推导；AI 推理依据与客观事实分层展示，用户可对照原帖核验。
 * 同一页面兼容四边停靠：宽停靠完整展示，窄停靠由 CSS 压缩次要信息（不缩字号、不裁按钮）。
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
  ipcErrorMessage,
  openExternalUrl,
  translateRadarPost,
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
  quotaBadgeLabel,
  quotaCorrelationLabel,
  radarCloseReasonLabel,
  radarDecisionBadge,
  radarDecisionTimeText,
  radarDeltaImpactLine,
  radarQuotaSummaryLine,
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
  const [quotaDetailOpen, setQuotaDetailOpen] = useState(false);
  const [recentEventOpen, setRecentEventOpen] = useState(false);

  const event = radar?.event ?? null;
  const decision = radar?.decision ?? null;
  const recentEvent = decision?.recentEvent ?? null;
  const knownPosts = radar?.posts ?? [];
  const verifications = radar?.quotaVerifications ?? [];
  const quotaSummary = radarQuotaSummaryLine(verifications);
  const evidencePosts = event
    ? knownPosts.filter((post) => event.postIds.includes(post.id))
    : [];
  // 相关动态：仅活动事件关联帖；最新无关帖以单条“最新动态”补充，不进入判断依据。
  const latestExtraPost =
    event && knownPosts[0] && !event.postIds.includes(knownPosts[0].id) ? knownPosts[0] : null;
  const aiAnalysis =
    radar?.aiAssessment.eventAnalysis ?? radar?.aiAssessment.latestDeltaAnalysis ?? null;

  // 分析范围为只读标签：修改入口在主窗口 AI 辅助分析页，悬浮页不提供第二套选择器。
  const rangeKey = radar?.analysisPrefs.rangeKey;
  const rangeTitle =
    radar?.analysisPrefs.analyze
      ? "当前检查将使用主窗口中保存的分析范围"
      : "仅影响启用 AI 后的分析输入；来源公告与帖子同步不受范围限制";

  const source = radar?.sourceAssessment;
  const sourceText =
    source?.headline ?? radar?.notice?.headline ?? "暂未同步来源内容";
  const sourceSub = source?.lead ?? null;

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

      {/* 1. 重置判断：第一屏直接回答“什么时候” */}
      <section className="hb-radar-card">
        <div className="hb-radar-card-head">
          <h3 className="hb-radar-card-title">重置判断</h3>
          {decision ? (
            <span className="radar-phase-badge" data-phase={decision.status}>
              {radarDecisionBadge(decision)}
            </span>
          ) : null}
        </div>
        {decision ? (
          <>
            <p className="hb-radar-text" data-selectable="true">
              {decision.headline}
            </p>
            <p className="hb-radar-meta" data-time-kind={decision.timeKind}>
              {radarDecisionTimeText(decision)}
            </p>
            {decision.status === "landed_observed" && decision.observationExpiresAt ? (
              <p className="hb-radar-meta" data-observation="true">
                处于 24 小时观察期 · 至 {formatHoverbarClock(decision.observationExpiresAt)}
              </p>
            ) : null}
            {radar ? <p className="hb-radar-meta">{radarDeltaImpactLine(radar)}</p> : null}
            {decision.status === "no_signal" && decision.recentEvent?.observedResetAt ? (
              <p className="hb-radar-meta">
                最近一次 {formatHoverbarClock(decision.recentEvent.observedResetAt)} 本机观察到刷新
              </p>
            ) : null}
          </>
        ) : (
          <p className="hb-radar-meta">正在加载重置判断…</p>
        )}
      </section>

      {/* 2. 判断依据：事实证据（客观）与 AI 推理依据（推测）分层 */}
      <section className="hb-radar-card">
        <h3 className="hb-radar-card-title">判断依据</h3>
        <p className="hb-radar-field-label">事实证据</p>
        <div className="hb-radar-judge">
          <span className="hb-radar-strip-tag">来源</span>
          <div className="hb-radar-judge-body">
            <p className="hb-radar-text" data-selectable="true">
              {sourceText}
            </p>
            {sourceSub ? (
              <p className="hb-radar-meta" data-selectable="true">
                {sourceSub}
              </p>
            ) : null}
            {event?.claimedLandedAt ? (
              <p className="hb-radar-meta">
                来源称落地 {formatHoverbarClock(event.claimedLandedAt)}
              </p>
            ) : null}
            {decision?.observedAt ? (
              <p className="hb-radar-meta">
                本机额度刷新 {formatHoverbarClock(decision.observedAt)}
              </p>
            ) : null}
          </div>
        </div>
        {evidencePosts.length > 0 ? (
          evidencePosts.map((post) => (
            <EvidenceRow key={post.id} post={post} />
          ))
        ) : (
          <p className="hb-radar-meta">暂无关联原帖。</p>
        )}
        <p className="hb-radar-field-label">AI 推理依据</p>
        <div className="hb-radar-legend">
          <span>原帖明确 = 来源可直接支持</span>
          <span>AI 推断 = 模型语境解读</span>
          <span>不确定性 = 未被验证</span>
        </div>
        {radar?.aiAssessment.enabled ? (
          aiAnalysis ? (
            <AiReasoningBlock
              analysis={aiAnalysis}
              posts={knownPosts}
              rangeKey={rangeKey ?? null}
              customPrompt={Boolean(
                radar.analysisPrefs.userPrompt.trim() &&
                  radar.analysisPrefs.userPrompt !== radar.analysisPrefs.defaultUserPrompt,
              )}
              title={
                radar.aiAssessment.eventAnalysis
                  ? "本轮事件分析"
                  : "最新动态分析"
              }
            />
          ) : (
            <p className="hb-radar-meta">开启 AI 后随「立即检查」生成分析。</p>
          )
        ) : (
          <p className="hb-radar-meta">AI 未启用：仅展示来源与本机事实。</p>
        )}
        {radar?.aiAssessment.latestError ? (
          <p className="hb-radar-error">{radar.aiAssessment.latestError}</p>
        ) : null}
      </section>

      {/* 3. 本机验证摘要：默认收缩，账号与窗口细节在「查看验证详情」 */}
      <section className="hb-radar-card">
        <div className="hb-radar-card-head">
          <h3 className="hb-radar-card-title">本机额度验证</h3>
          {verifications.length > 0 ? (
            <button
              type="button"
              className="hb-radar-post-button"
              onClick={() => setQuotaDetailOpen((open) => !open)}
            >
              {quotaDetailOpen ? "收起验证详情" : "查看验证详情"}
              <ChevronDown
                size={12}
                aria-hidden
                className={quotaDetailOpen ? "hb-rotate-180" : ""}
              />
            </button>
          ) : null}
        </div>
        <p className="hb-radar-text" data-selectable="true">
          {quotaSummary}
        </p>
        {quotaDetailOpen ? (
          <>
            {verifications.length === 0 ? (
              <p className="hb-radar-meta">未接入 GPT 额度来源。</p>
            ) : (
              verifications.map((item) => <QuotaVerificationRow key={item.sourceId} item={item} />)
            )}
          </>
        ) : null}
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

      {/* 4. 时间线：仅有节点时渲染，不显示空卡 */}
      {event && event.timeline.length > 0 ? (
        <section className="hb-radar-card">
          <h3 className="hb-radar-card-title">时间线</h3>
          <div className="hb-timeline">
            {event.timeline.map((node) => (
              <div className="hb-timeline-node" key={`${node.kind}-${node.at}`}>
                <span className="hb-timeline-dot" data-kind={node.kind} aria-hidden />
                <span className="hb-timeline-label">{node.label}</span>
                <span className="hb-timeline-time">{formatHoverbarClock(node.at)}</span>
              </div>
            ))}
          </div>
        </section>
      ) : null}

      {/* 5. 相关动态：无活动事件时不再回退最新三帖 */}
      {event ? (
        <section className="hb-radar-card">
          <h3 className="hb-radar-card-title">相关动态</h3>
          {evidencePosts.length === 0 ? (
            <p className="hb-radar-meta">暂无关联动态。</p>
          ) : (
            evidencePosts.map((post) => (
              <RadarPostItem
                key={post.id}
                post={post}
                translating={translate.isPending && translate.variables === post.id}
                onTranslate={() => translate.mutate(post.id)}
              />
            ))
          )}
          {latestExtraPost ? (
            <>
              <p className="hb-radar-field-label">最新动态（与事件无关）</p>
              <RadarPostItem
                post={latestExtraPost}
                translating={translate.isPending && translate.variables === latestExtraPost.id}
                onTranslate={() => translate.mutate(latestExtraPost.id)}
              />
            </>
          ) : null}
          {translate.error ? (
            <p className="hb-radar-error">{ipcErrorMessage(translate.error, "翻译失败")}</p>
          ) : null}
        </section>
      ) : null}

      {/* 6. 最近一次事件：仅无活动事件时展示，不冒充当前信号 */}
      {!event && recentEvent ? (
        <section className="hb-radar-card">
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
            最近一次事件
            <span className="hb-radar-card-toggle-meta">
              {recentEvent.observedResetAt
                ? `${formatHoverbarClock(recentEvent.observedResetAt)} 观察到刷新`
                : radarCloseReasonLabel(recentEvent.closeReason)}
            </span>
            <ChevronDown size={13} aria-hidden className={recentEventOpen ? "hb-rotate-180" : ""} />
          </button>
          {recentEventOpen ? (
            <div className="hb-radar-recent-body">
              <p className="hb-radar-text" data-selectable="true">
                {humanizeRadarPostRefs(recentEvent.title, knownPosts)}
              </p>
              <p className="hb-radar-meta">
                最终状态：{radarCloseReasonLabel(recentEvent.closeReason)}
                {recentEvent.observedResetAt
                  ? ` · 观察于 ${formatHoverbarClock(recentEvent.observedResetAt)}`
                  : null}
              </p>
              {recentEvent.analysis ? (
                <>
                  <p className="hb-radar-field-label">当时的分析</p>
                  {recentEvent.analysis.conclusion ? (
                    <p className="hb-radar-text" data-selectable="true">
                      {humanizeRadarPostRefs(recentEvent.analysis.conclusion, knownPosts)}
                    </p>
                  ) : null}
                  {recentEvent.analysis.analysisBasis ? (
                    <p className="hb-radar-meta" data-selectable="true">
                      {humanizeRadarPostRefs(
                        recentEvent.analysis.analysisBasis,
                        knownPosts,
                      )}
                    </p>
                  ) : null}
                </>
              ) : null}
              <div className="hb-radar-recent-posts">
                {knownPosts
                  .filter((post) => recentEvent.postIds.includes(post.id))
                  .map((post) => (
                    <EvidenceRow key={post.id} post={post} />
                  ))}
              </div>
            </div>
          ) : null}
        </section>
      ) : null}

      {refreshError ? <p className="hb-radar-error">{refreshError}</p> : null}
      <p className="hb-radar-footnote">仅为推测，不代表官方结论；重置时间以官方实际执行为准。</p>
    </div>
  );
}

/** 事实证据中的原帖行：英文摘录 + Rust 解析的北京时间 + 查看原帖，不含 AI 推理文案。 */
function EvidenceRow({ post }: { post: RadarPost }) {
  const [linkError, setLinkError] = useState<string | null>(null);
  return (
    <div className="hb-evidence-row">
      <div className="hb-evidence-main">
        <p className="hb-evidence-text" data-selectable="true">
          {post.summary ?? post.text}
        </p>
        <span className="hb-evidence-time">{formatHoverbarClock(post.postedAt)}</span>
      </div>
      {post.url ? (
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
          <ExternalLink size={12} aria-hidden />
          查看原帖
        </button>
      ) : null}
      {linkError ? <p className="hb-radar-error">{linkError}</p> : null}
    </div>
  );
}

/** AI 推理依据块：结论 + 分析依据（默认可见）+ 支持/反向/不确定性 + 引用与元信息。 */
function AiReasoningBlock({
  analysis,
  posts,
  rangeKey,
  customPrompt,
  title,
}: {
  analysis: RadarAnalysis;
  posts: RadarPost[];
  rangeKey: string | null;
  customPrompt: boolean;
  title: string;
}) {
  const byId = new Map(posts.map((post) => [post.id, post]));
  const citationPosts = analysis.citations
    .map((id) => byId.get(id))
    .filter((post): post is RadarPost => Boolean(post));
  return (
    <div className="hb-ai-block">
      <p className="hb-radar-meta">
        {title}
        {analysis.model ? ` · 模型 ${analysis.model}` : ""}
        {` · ${formatHoverbarClock(analysis.createdAt)}`}
        {rangeKey ? ` · 范围 ${formatRadarRangeLabel(rangeKey, true)}` : ""}
        {` · ${customPrompt ? "自定义语义提示" : "默认提示"}`}
      </p>
      {analysis.conclusion ? (
        <p className="hb-radar-text" data-selectable="true">
          {analysis.conclusion}
        </p>
      ) : null}
      {analysis.analysisBasis ? (
        <p className="hb-radar-meta" data-selectable="true">
          {analysis.analysisBasis}
        </p>
      ) : null}
      {analysis.support.length > 0 ? (
        <>
          <p className="hb-radar-field-label">原帖明确（支持）</p>
          {analysis.support.map((item, index) => (
            <p className="hb-radar-meta" data-selectable="true" key={`support-${index}`}>
              · {item}
            </p>
          ))}
        </>
      ) : null}
      {analysis.against.length > 0 ? (
        <>
          <p className="hb-radar-field-label">反向依据</p>
          {analysis.against.map((item, index) => (
            <p className="hb-radar-meta" data-selectable="true" key={`against-${index}`}>
              · {item}
            </p>
          ))}
        </>
      ) : null}
      {analysis.uncertainty.length > 0 ? (
        <>
          <p className="hb-radar-field-label">不确定性</p>
          {analysis.uncertainty.map((item, index) => (
            <p className="hb-radar-meta" data-selectable="true" key={`uncertain-${index}`}>
              · {item}
            </p>
          ))}
        </>
      ) : null}
      {citationPosts.length > 0 ? (
        <div className="hb-citation-row">
          {citationPosts.map((post) => (
            <CitationChip key={post.id} post={post} />
          ))}
        </div>
      ) : null}
    </div>
  );
}

/** 引用原帖入口：时间 + 查看原帖，不显示原始帖子编号。 */
function CitationChip({ post }: { post: RadarPost }) {
  const [linkError, setLinkError] = useState<string | null>(null);
  return (
    <span className="hb-citation-chip">
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
