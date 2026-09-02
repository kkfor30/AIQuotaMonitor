import { type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  BrainCircuit,
  ChevronDown,
  ChevronRight,
  ExternalLink,
  Heart,
  HelpCircle,
  Link2,
  MessageCircle,
  Radar,
  RefreshCw,
  Repeat2,
  ShieldCheck,
  ThumbsDown,
  ThumbsUp,
} from "lucide-react";
import { Button } from "@/components/ui/Button";
import { Switch } from "@/components/ui/Switch";
import { StatusBadge } from "@/components/ui/StatusBadge";
import { cn } from "@/lib/cn";
import {
  addRadarCustomModel,
  cancelRadarCheck,
  confirmRadarQuotaChange,
  confirmRadarUserReset,
  deleteRadarCustomModel,
  fetchRadarSnapshot,
  ipcErrorMessage,
  openExternalUrl,
  runRadarCheck,
  saveRadarAnalysisPrefs,
  testRadarModel,
  undoRadarUserReset,
} from "@/lib/ipc";
import { RADAR_SNAPSHOT_QUERY_KEY } from "@/lib/query-client";
import type { RadarModelOption, RadarPost, RadarSnapshot } from "@/lib/ipc";
import {
  formatRadarRangeLabel,
  humanizeRadarPostRefs,
  postsInRadarRange,
  quotaBadgeLabel,
  quotaCorrelationLabel,
  radarAiStatusLabel,
  radarBeijingTimeLabel,
  radarCloseReasonLabel,
  radarConfirmationSourceLabel,
  radarDecisionBadge,
  radarDecisionTimeText,
  radarDeltaImpactLine,
  radarQuotaSummaryLine,
  sourceRelationLabel,
} from "@/features/hoverbar/hoverbar-state";
import { useContainerWidth, TIBO_SPLIT_MIN_PX } from "@/lib/use-container-width";
import { ArrowLeft } from "lucide-react";

export function GptRadarPage() {
  const queryClient = useQueryClient();
  const [tab, setTab] = useState<RadarTabId>("signal");
  const [filter, setFilter] = useState<"all" | "signal" | "related" | "none">("all");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [rangeKey, setRangeKey] = useState("3d");
  const [analyze, setAnalyze] = useState(false);
  const [sourceId, setSourceId] = useState<string>("");
  const [modelChoice, setModelChoice] = useState<string>("");
  const [userPrompt, setUserPrompt] = useState("");

  const { data, isLoading } = useQuery({
    queryKey: RADAR_SNAPSHOT_QUERY_KEY,
    queryFn: fetchRadarSnapshot,
  });
  // 跨窗口检查状态：悬浮窗触发检查时主窗口按钮同步 loading。
  const [externalChecking, setExternalChecking] = useState(false);
  useEffect(() => {
    const unlisteners: Array<() => void> = [];
    let disposed = false;
    void listen("radar-check-started", () => setExternalChecking(true)).then((unlisten) =>
      disposed ? unlisten() : unlisteners.push(unlisten),
    );
    void listen("radar-check-finished", () => setExternalChecking(false)).then((unlisten) =>
      disposed ? unlisten() : unlisteners.push(unlisten),
    );
    return () => {
      disposed = true;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, []);
  const prefsReady = useRef(false);

  useEffect(() => {
    if (!data || prefsReady.current) return;
    prefsReady.current = true;
    setAnalyze(data.analysisPrefs.analyze);
    setRangeKey(data.analysisPrefs.rangeKey || "3d");
    if (data.analysisPrefs.sourceId) setSourceId(data.analysisPrefs.sourceId);
    if (data.analysisPrefs.model) setModelChoice(data.analysisPrefs.model);
    setUserPrompt(data.analysisPrefs.userPrompt ?? data.analysisPrefs.defaultUserPrompt ?? "");
  }, [data]);

  useEffect(() => {
    if (!prefsReady.current) return;
    const timer = window.setTimeout(() => {
      void saveRadarAnalysisPrefs({
        analyze,
        rangeKey,
        sourceId: sourceId || null,
        model: modelChoice || null,
        userPrompt,
      });
    }, 200);
    return () => window.clearTimeout(timer);
  }, [analyze, rangeKey, sourceId, modelChoice, userPrompt]);

  // 所选来源 + 模型的选项；modelChoice 失配（来源切换、旧偏好）时回退该来源默认模型。
  const modelOptions = data?.models ?? [];
  const chosenModel =
    modelOptions.find((item) => item.sourceId === sourceId && item.model === modelChoice) ??
    modelOptions.find((item) => item.sourceId === sourceId) ??
    null;

  const checkMutation = useMutation({
    mutationFn: () =>
      runRadarCheck({
        analyze,
        rangeKey,
        sourceId: chosenModel?.sourceId ?? null,
        model: chosenModel?.model ?? null,
        userPrompt,
      }),
    onSuccess: (snapshot) => {
      queryClient.setQueryData(RADAR_SNAPSHOT_QUERY_KEY, snapshot);
    },
  });

  const radarChecking = checkMutation.isPending || externalChecking;
  const checkCancelled = checkMutation.error
    ? ipcErrorMessage(checkMutation.error, "检查失败").includes("已终止")
    : false;
  const posts = postsInRadarRange(data?.posts ?? [], rangeKey).slice().sort((a, b) => b.postedAt - a.postedAt);
  const visible = posts.filter((post) => {
    if (filter === "all") return true;
    if (filter === "signal") return post.explicitReset || post.filter === "signal";
    if (filter === "related") return post.filter === "related";
    return post.filter === "none" && !post.explicitReset;
  });
  const selected = visible.find((post) => post.id === selectedId) ?? visible[0] ?? null;

  // 动态范围（复用 analysisPrefs.rangeKey）：同时控制 Tibo 列表与 AI 上下文，AI 关闭时仍可设置。
  const customRange = customRangeOf(rangeKey);
  const customActive = !QUICK_RANGES.some((range) => range.id === rangeKey) && customRange !== null;
  const todayIso = toIsoDate(new Date());
  // 记住最近一次自定义区间：切到快捷档再切回「自定义」时恢复，而不是重置成默认 30 天
  const lastCustomRangeRef = useRef<string | null>(
    customRange ? `range:${customRange.start}:${customRange.end}` : null,
  );
  const applyCustomRange = (start: string, end: string) => {
    if (!start || !end || start.length !== 10 || end.length !== 10) return;
    const [from, to] = start <= end ? [start, end] : [end, start];
    lastCustomRangeRef.current = `range:${from}:${to}`;
    setRangeKey(`range:${from}:${to}`);
  };
  const switchToCustom = () => {
    setRangeKey(lastCustomRangeRef.current ?? defaultCustomRangeKey());
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-hidden p-4 pt-2 pr-2">
      {/* 页头：单行紧凑条——按钮固定右上；窄窗口时仅说明文字收缩截断，布局不随宽度换行 */}
      <header className="glass-panel flex shrink-0 items-center gap-2.5 px-4 py-2.5">
        <span
          aria-hidden
          className="flex h-8 w-8 shrink-0 items-center justify-center rounded-[10px] border border-q-border bg-q-surface-strong text-q-primary shadow-q-sm"
        >
          <Radar size={16} aria-hidden />
        </span>
        <h1 className="shrink-0 text-[17px] font-semibold tracking-tight text-q-text-primary">GPT 重置雷达</h1>
        <span className="shrink-0 rounded-q-pill bg-q-neutral-soft px-2 py-0.5 text-[11px] text-q-neutral">
          仅为推测，不代表官方结论
        </span>
        <p className="min-w-0 flex-1 truncate text-[12.5px] text-q-text-secondary">
          手动同步 CodexRadar 公开首页的 Tibo 动态与中文翻译；AI 分析默认关闭，只使用英文原文。
        </p>
      </header>
      {checkMutation.error && !checkCancelled && (
        <p className="rounded-q-control border border-q-danger/25 bg-q-danger-soft px-3 py-2 text-xs text-q-danger">
          {ipcErrorMessage(checkMutation.error, "检查失败")}
        </p>
      )}

      {/* 分段 Tab + 立即检查 + 动态范围（同一行公开可见；范围同时控制 Tibo 列表与 AI 上下文） */}
      <div className="flex shrink-0 flex-wrap items-center justify-between gap-x-4 gap-y-2">
        <div className="flex min-w-0 items-center gap-3">
          <div
            role="tablist"
            className="inline-flex w-fit items-center gap-1 rounded-q-pill border border-q-border bg-q-surface-muted p-1"
          >
            {RADAR_TABS.map((item) => (
              <button
                key={item.id}
                type="button"
                role="tab"
                aria-selected={tab === item.id}
                onClick={() => setTab(item.id)}
                data-active={tab === item.id}
                className={cn(
                  "cursor-pointer rounded-q-pill px-4 py-1.5 text-[13px] font-medium transition-colors duration-150",
                  "text-q-text-secondary hover:text-q-text-primary",
                  "data-[active=true]:bg-[var(--q-chip-active-bg)] data-[active=true]:text-[var(--q-chip-active-text)] data-[active=true]:shadow-q-sm",
                )}
              >
                {item.label}
              </button>
            ))}
          </div>
          {radarChecking ? (
            <Button variant="ghost" size="sm" className="shrink-0" onClick={() => void cancelRadarCheck()}>
              <RefreshCw size={14} aria-hidden className="animate-spin" />
              终止检查
            </Button>
          ) : (
            <Button size="sm" className="shrink-0" onClick={() => checkMutation.mutate()}>
              <RefreshCw size={14} aria-hidden />
              立即检查
            </Button>
          )}
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <span className="text-[13px] font-medium text-q-text-secondary">动态范围：</span>
          {QUICK_RANGES.map((range) => (
            <button
              key={range.id}
              type="button"
              aria-pressed={rangeKey === range.id}
              onClick={() => setRangeKey(range.id)}
              className={rangeChipClass(rangeKey === range.id)}
            >
              {range.label}
            </button>
          ))}
          <button
            type="button"
            aria-pressed={customActive}
            onClick={() => {
              if (!customActive) switchToCustom();
            }}
            className={rangeChipClass(customActive)}
          >
            自定义
          </button>
          {customActive && customRange && (
            <div className="flex flex-wrap items-center gap-2 rounded-q-control border border-q-border bg-q-surface-strong px-3 py-1.5">
              <input
                type="date"
                value={customRange.start}
                max={todayIso}
                onChange={(event) => applyCustomRange(event.target.value, customRange.end)}
                aria-label="动态范围开始日期"
                className={dateInputClass}
              />
              <span className="text-xs text-q-text-muted">至</span>
              <input
                type="date"
                value={customRange.end}
                max={todayIso}
                onChange={(event) => applyCustomRange(customRange.start, event.target.value)}
                aria-label="动态范围结束日期"
                className={dateInputClass}
              />
            </div>
          )}
        </div>
      </div>

      {isLoading && <p className="shrink-0 text-sm text-q-text-muted">正在加载雷达数据…</p>}
      {tab === "signal" && <SignalSummaryView data={data} />}
      {tab === "tibo" && (
        <TiboFeedView
          posts={visible}
          rangeLabel={formatRadarRangeLabel(rangeKey)}
          totalCount={posts.length}
          signalCount={posts.filter((p) => p.explicitReset || p.filter === "signal").length}
          relatedCount={posts.filter((p) => p.filter === "related").length}
          noneCount={posts.filter((p) => p.filter === "none" && !p.explicitReset).length}
          selected={selected}
          filter={filter}
          onFilter={setFilter}
          onSelect={setSelectedId}
        />
      )}
      {tab === "ai" && (
        <AiAnalysisView
          data={data}
          analyze={analyze}
          sourceId={sourceId || modelOptions.find((item) => item.ready)?.sourceId || ""}
          modelChoice={modelChoice}
          models={modelOptions}
          onAnalyzeChange={setAnalyze}
          onSourceChange={setSourceId}
          onModelChange={setModelChoice}
          userPrompt={userPrompt}
          defaultUserPrompt={data?.analysisPrefs.defaultUserPrompt ?? ""}
          onUserPromptChange={setUserPrompt}
        />
      )}
    </div>
  );
}

type RadarTabId = "signal" | "tibo" | "ai";
const RADAR_TABS: Array<{ id: RadarTabId; label: string }> = [
  { id: "signal", label: "信号摘要" },
  { id: "tibo", label: "Tibo 动态" },
  { id: "ai", label: "AI 辅助分析" },
];

/** 自定义日期输入样式（Tab 行动态范围控件用）。 */
const dateInputClass =
  "h-8 rounded-md border border-q-border bg-q-surface px-2 text-xs text-q-text-primary outline-none focus:border-q-primary";

function formatTime(value: number) {
  return new Date(value).toLocaleString("zh-CN", { hour12: false });
}

function formatCompactTime(value: number) {
  return new Date(value).toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  });
}

function AnimatedCollapse({ open, children, className = "" }: { open: boolean; children: ReactNode; className?: string }) {
  return (
    <div className={`radar-collapse ${className}`} data-open={open || undefined} aria-hidden={!open}>
      <div className="radar-collapse-inner">{children}</div>
    </div>
  );
}

/** 本机额度验证状态的徽章色调：非计划刷新 → 蓝（重点）；不可达/缺数据 → 中性；其余正常。 */
function quotaTone(status: string): "success" | "warning" | "neutral" | "primary" {
  if (status === "unscheduled_reset" || status === "possible_reset") return "primary";
  if (status === "scheduled" || status === "no_change") return "success";
  return "neutral";
}

/** 帖子信号徽章色调：显式重置 → 红；信号/相关 → 橙；无信号 → 灰 */
function postBadgeTone(post: RadarPost): "danger" | "warning" | "neutral" | "primary" {
  if (post.explicitReset) return "danger";
  if (post.filter === "signal") return "warning";
  if (post.filter === "related") return "primary";
  if (post.filter === "none") return "neutral";
  return "primary";
}

/* ————————————————— 信号摘要（设计稿 04） ————————————————— */

function SignalSummaryView({
  data,
}: {
  data: RadarSnapshot | undefined;
}) {
  const queryClient = useQueryClient();
  const decision = data?.decision ?? null;
  // “最近一次重置”只认本机观察/用户确认事件；普通关闭事件仅在无确认重置时兜底展示为来源声称。
  const recentReset = decision?.recentReset ?? null;
  const recentClosedEvent = decision?.recentClosedEvent ?? null;
  const recentCard = recentReset ?? recentClosedEvent;
  const knownPosts = data?.posts ?? [];
  const ai = data?.aiAssessment;
  const verifications = data?.quotaVerifications ?? [];
  // 当前分析选择规则：有活动事件优先 eventAnalysis，无活动事件只读 latestDeltaAnalysis。
  const aiReasoning = ai?.eventAnalysis ?? ai?.latestDeltaAnalysis ?? null;
  const citedIds = decision?.currentKeyCitationIds ?? [];
  const citedPosts = knownPosts.filter((post) => citedIds.includes(post.id));
  const notice = data?.notice ?? null;
  const aiStatusLabel = radarAiStatusLabel(ai);
  // AI 关闭/历史态：确定性判断与 Tibo 数据继续展示，旧结果只能作为历史结果折叠查看。
  const showCurrentAnalysis = Boolean(ai?.enabled && ai.state !== "historical" && aiReasoning?.conclusion);
  const historicalAnalysis = ai?.history ?? aiReasoning;
  const [recentOpen, setRecentOpen] = useState(false);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [analysisOpen, setAnalysisOpen] = useState(false);
  const [historyAnalysisOpen, setHistoryAnalysisOpen] = useState(false);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const recentResetAt = recentReset?.observedResetAt ?? recentReset?.userConfirmedResetAt ?? null;
  const recentMeta = recentResetAt
    ? `${formatCompactTime(recentResetAt)} · ${radarConfirmationSourceLabel(recentReset?.confirmationSource)}`
    : decision?.recentSummaryText ?? "已结束";
  // 本机观察到刷新且无用户确认/时间关联时补充“原因未知”。
  const observedCauseUnknown =
    decision?.status === "landed_observed" &&
    !verifications.some(
      (item) => item.attribution === "user_confirmed" || Boolean(quotaCorrelationLabel(item.temporalCorrelation)),
    );
  // 本机验证汇总徽章：表达整体可用性，不冒充判断结论。
  const quotaOverall = (() => {
    if (verifications.length === 0) return null;
    if (verifications.every((item) => item.status === "unavailable")) {
      return { tone: "neutral" as const, label: "暂无法验证" };
    }
    if (verifications.some((item) => ["unscheduled_reset", "possible_reset"].includes(item.status))) {
      return { tone: "warning" as const, label: "有待确认" };
    }
    return { tone: "success" as const, label: "正常" };
  })();
  const quotaSummary = radarQuotaSummaryLine(verifications);
  const confirmCard = useMutation({
    mutationFn: confirmRadarQuotaChange,
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

  return (
    <div className="radar-console flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto pr-1">
      {/* 两张状态卡：判断（含最近一次重置）+ 本机验证（宽屏等高，窄屏自动换行） */}
      <div className="radar-status-grid shrink-0">
        <section className="glass-panel flex flex-col gap-2 p-4">
          <div className="flex items-center gap-2">
            <h2 className="text-[13px] font-semibold tracking-tight text-q-text-secondary">当前判断</h2>
            {decision ? (
              <span className="rounded-q-pill bg-q-primary-soft px-2 py-0.5 text-[11px] font-medium text-q-primary">
                {radarDecisionBadge(decision)}
              </span>
            ) : null}
            {data ? (
              <span className="ml-auto shrink-0 tabular-nums text-[11px] text-q-text-muted">
                更新 {formatCompactTime(data.lastSyncedAt ?? Date.now())}
              </span>
            ) : null}
          </div>
          {decision ? (
            <>
              <p className="text-[16px] font-semibold leading-snug text-q-text-primary" data-selectable="true">
                {decision.headline}
              </p>
              <p className="text-[12.5px] font-medium text-q-text-secondary">{radarDecisionTimeText(decision)}</p>
              {observedCauseUnknown ? <p className="text-[12px] text-q-text-secondary">原因未知</p> : null}
              {decision.verificationHint ? (
                <p className="text-[12px] leading-relaxed text-q-text-secondary">{decision.verificationHint}</p>
              ) : null}
              {decision.observationPeriodText ? (
                <p className="text-[12px] font-medium text-q-success">{decision.observationPeriodText}</p>
              ) : null}
              {data ? <p className="text-[12px] leading-relaxed text-q-text-secondary">{radarDeltaImpactLine(data)}</p> : null}
              {decision.status === "no_signal" && decision.recentSummaryText ? (
                <p className="text-[11.5px] text-q-text-muted">{decision.recentSummaryText}</p>
              ) : null}
              {decision.canConfirmReset || decision.canUndoConfirm ? (
                <div className="flex flex-wrap items-center gap-2 pt-0.5">
                  {decision.canConfirmReset ? (
                    <Button variant="secondary" size="sm" onClick={() => setConfirmOpen(true)}>
                      确认额度已重置
                    </Button>
                  ) : null}
                  {decision.canUndoConfirm ? (
                    <Button variant="ghost" size="sm" disabled={undoReset.isPending} onClick={() => undoReset.mutate()}>
                      撤销人工确认
                    </Button>
                  ) : null}
                </div>
              ) : null}
            </>
          ) : (
            <p className="text-xs text-q-text-muted">正在加载重置判断…</p>
          )}

          {/* 最近一次重置：并入判断卡的折叠小节，只认本机观察/用户确认；展开保留 时间/确认方式/最终状态/当时结论/当时分析 */}
          {recentCard ? (
            <div className="mt-auto flex flex-col border-t border-q-border/70 pt-2">
              <button
                type="button"
                className="radar-collapse-trigger"
                onClick={() => setRecentOpen((value) => !value)}
                aria-expanded={recentOpen}
              >
                <ChevronRight
                  size={17}
                  aria-hidden
                  className={cn("radar-collapse-chevron", recentOpen && "is-open")}
                />
                <h2 className="text-[15px] font-semibold tracking-tight text-q-text-primary">
                  {recentReset ? "最近一次重置" : "最近一次事件"}
                </h2>
                <span className="min-w-0 flex-1 truncate text-[13px]">
                  {recentResetAt ? (
                    <>
                      <span className="font-semibold tabular-nums text-q-text-primary">
                        {formatCompactTime(recentResetAt)}
                      </span>
                      <span className="text-q-text-muted">
                        {" "}
                        · {radarConfirmationSourceLabel(recentReset?.confirmationSource)}
                      </span>
                    </>
                  ) : (
                    <span className="text-q-text-muted">{recentMeta}</span>
                  )}
                </span>
              </button>
              <AnimatedCollapse open={recentOpen}>
                <div className="flex flex-col gap-1.5 pt-2">
                  {recentResetAt ? (
                    <p className="text-[13px] leading-relaxed text-q-text-secondary">
                      <span className="font-semibold text-q-text-primary">真实时间：</span>
                      <span className="tabular-nums">{formatCompactTime(recentResetAt)}</span>
                    </p>
                  ) : null}
                  <p className="text-[13px] leading-relaxed text-q-text-secondary">
                    <span className="font-semibold text-q-text-primary">确认方式：</span>
                    {radarConfirmationSourceLabel(recentCard.confirmationSource)}
                  </p>
                  <p className="text-[13px] leading-relaxed text-q-text-secondary">
                    <span className="font-semibold text-q-text-primary">最终状态：</span>
                    {radarCloseReasonLabel(recentCard.closeReason)}
                  </p>
                  {recentCard.analysis?.conclusion ? (
                    <p className="text-[13px] leading-relaxed text-q-text-secondary" data-selectable="true">
                      <span className="font-semibold text-q-text-primary">当时结论：</span>
                      {humanizeRadarPostRefs(recentCard.analysis.conclusion, knownPosts)}
                    </p>
                  ) : null}
                  {recentCard.analysis?.analysisBasis ? (
                    <p className="text-[13px] leading-relaxed text-q-text-secondary" data-selectable="true">
                      <span className="font-semibold text-q-text-primary">当时分析：</span>
                      {humanizeRadarPostRefs(recentCard.analysis.analysisBasis, knownPosts)}
                    </p>
                  ) : null}
                </div>
              </AnimatedCollapse>
            </div>
          ) : null}
        </section>

        {/* 本机验证：汇总徽章 + 一句汇总；账号详情直接展开，多账号时卡内滚动防溢出 */}
        <section className="glass-panel flex flex-col gap-2 p-4">
          <div className="flex items-center gap-2">
            <h2 className="text-[13px] font-semibold tracking-tight text-q-text-secondary">本机验证</h2>
            {quotaOverall ? <StatusBadge tone={quotaOverall.tone}>{quotaOverall.label}</StatusBadge> : null}
          </div>
          {verifications.length === 0 ? (
            <p className="text-xs text-q-text-muted">未接入 GPT 额度来源。</p>
          ) : (
            <>
              <p className="text-[12.5px] leading-relaxed text-q-text-secondary" data-selectable="true">
                {quotaSummary}
              </p>
              <div className="radar-verification-scroll flex flex-col">
                {verifications.map((item, index) => (
                  <div
                    key={item.sourceId}
                    className={cn("flex flex-col gap-1 py-2", index > 0 && "border-t border-q-border/60")}
                  >
                    <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
                      <b className="whitespace-nowrap text-[13px] text-q-text-primary">{item.accountName}</b>
                      <StatusBadge tone={quotaTone(item.status)}>
                        {quotaBadgeLabel(item.status, item.attribution, item.lastResetObservedAt)}
                      </StatusBadge>
                      {item.attribution === "user_confirmed" ? (
                        <span className="text-[11px] text-q-text-muted">用户已确认重置卡</span>
                      ) : null}
                      {quotaCorrelationLabel(item.temporalCorrelation) ? (
                        <span className="text-[11px] text-q-text-muted">{quotaCorrelationLabel(item.temporalCorrelation)}</span>
                      ) : null}
                    </div>
                    {item.status === "unavailable" && (
                      <p className="text-[11.5px] leading-relaxed text-q-text-secondary">
                        当前网络无法获取 Codex 额度，不影响来源与 AI 判断。
                      </p>
                    )}
                    {item.note && <p className="text-[11.5px] leading-relaxed text-q-text-secondary">{item.note}</p>}
                    <p className="text-[11.5px] tabular-nums text-q-text-muted">
                      {item.windowLabel ? `${item.windowLabel} · ` : ""}
                      {item.lastSuccessAt ? `上次成功 ${formatCompactTime(item.lastSuccessAt)}` : "尚无成功快照"}
                    </p>
                    {["unscheduled_reset", "possible_reset"].includes(item.status) &&
                      item.observationId != null &&
                      item.attribution !== "user_confirmed" && (
                        <Button
                          variant="ghost"
                          size="sm"
                          className="self-start"
                          disabled={confirmCard.isPending}
                          onClick={() =>
                            confirmCard.mutate({
                              observationId: item.observationId ?? 0,
                              confirmedAt: Date.now(),
                            })
                          }
                        >
                          确认这是我手动使用的重置卡
                        </Button>
                      )}
                  </div>
                ))}
              </div>
            </>
          )}
        </section>
      </div>

      {/* CodexRadar 公告：全宽细条；区分 当前公告 / 最近公告 / 当前无公告 / 缓存可能过期；不进入本地 AI 输入 */}
      <section className="glass-panel radar-notice-card flex shrink-0 flex-wrap items-center gap-x-3 gap-y-1 px-4 py-2.5">
        <h2 className="whitespace-nowrap text-[14px] font-semibold tracking-tight text-q-text-primary">
          {notice ? (notice.isCurrent ? "CodexRadar 公告" : "CodexRadar 最近公告") : "CodexRadar 当前无公告"}
        </h2>
        {notice ? (
          <span className="rounded-q-pill bg-q-primary-soft px-2 py-0.5 text-[11px] font-medium text-q-primary">
            {notice.isCurrent ? "当前公告" : "最近公告"}
          </span>
        ) : null}
        <span
          className={
            data?.sourceStatus === "fresh"
              ? "rounded-q-pill bg-q-success-soft px-2 py-0.5 text-[11px] font-medium text-q-success"
              : data?.sourceStatus === "stale"
                ? "rounded-q-pill bg-q-warning-soft px-2 py-0.5 text-[11px] font-medium text-q-warning"
                : "rounded-q-pill bg-q-neutral-soft px-2 py-0.5 text-[11px] text-q-neutral"
          }
        >
          {data?.sourceStatus === "fresh" ? "同步正常" : data?.sourceStatus === "stale" ? "缓存可能过期" : "尚未同步"}
        </span>
        {notice ? (
          <>
            <p className="min-w-0 flex-1 text-[13px] leading-relaxed text-q-text-secondary" data-selectable="true">
              <span className="font-semibold text-q-text-primary">{notice.headline}</span>
              {notice.lead ? <span> · {notice.lead}</span> : null}
            </p>
            <span className="ml-auto flex shrink-0 items-center gap-3 whitespace-nowrap text-[12px] text-q-text-muted">
              {notice.isCurrent
                ? notice.updatedAt
                  ? `更新 ${formatTime(notice.updatedAt)}`
                  : "当前公告"
                : notice.updatedAt
                  ? `上次出现于 ${formatTime(notice.updatedAt)}`
                  : "历史公告"}
              <a
                href="https://codexradar.com/"
                target="_blank"
                rel="noreferrer"
                className="inline-flex cursor-pointer items-center gap-1 text-q-primary hover:underline"
                onClick={(event) => {
                  event.preventDefault();
                  void openExternalUrl("https://codexradar.com/").catch(() => {});
                }}
              >
                <ExternalLink size={12} aria-hidden />
                打开 CodexRadar
              </a>
            </span>
          </>
        ) : null}
      </section>

      {/* AI 分析全宽主卡：结论/分析/正向依据；详情开关在正文底部 */}
      <section className="glass-panel flex shrink-0 flex-col gap-3 p-4">
        <div className="flex items-center gap-2.5">
          <h2 className="text-[16px] font-semibold tracking-tight text-q-text-primary">AI 分析</h2>
          <span
            className={
              !ai || ai.state === "disabled"
                ? "rounded-q-pill bg-q-neutral-soft px-2.5 py-1 text-[12px] font-medium text-q-neutral"
                : ai.state === "covered"
                  ? "rounded-q-pill bg-q-success-soft px-2.5 py-1 text-[12px] font-medium text-q-success"
                  : ai.state === "failed"
                    ? "rounded-q-pill bg-q-danger-soft px-2.5 py-1 text-[12px] font-medium text-q-danger"
                    : "rounded-q-pill bg-q-warning-soft px-2.5 py-1 text-[12px] font-medium text-q-warning"
            }
          >
            {aiStatusLabel}
          </span>
        </div>
        {showCurrentAnalysis && aiReasoning ? (
          <>
            <div className="flex flex-col gap-1.5">
              <p className="text-[13px] font-semibold text-q-primary">结论</p>
              <p className="text-[17px] font-semibold leading-relaxed text-q-text-primary" data-selectable="true">
                {humanizeRadarPostRefs(aiReasoning.conclusion, knownPosts)}
              </p>
            </div>
            {aiReasoning.analysisBasis ? (
              <div className="flex flex-col gap-1.5">
                <p className="text-[13px] font-semibold text-q-primary">分析</p>
                <p className="text-[14px] font-medium leading-[1.65] text-q-text-secondary" data-selectable="true">
                  {humanizeRadarPostRefs(aiReasoning.analysisBasis, knownPosts)}
                </p>
              </div>
            ) : null}
            <div className="flex flex-col gap-1.5">
              <p className="text-[13px] font-semibold text-q-primary">正向依据</p>
              {citedPosts.length > 0 ? (
                <div className="flex flex-col gap-2">
                  {citedPosts.map((post) => (
                    <div key={post.id} className="rounded-q-control border border-q-border bg-q-surface-strong px-3 py-2">
                      <div className="flex items-center gap-2">
                        <span
                          className={
                            aiReasoning.eventRelation === "none"
                              ? "hb-signal-tag hb-signal-tag-none"
                              : "hb-signal-tag hb-signal-tag-direct"
                          }
                        >
                          {aiReasoning.eventRelation === "none" ? "无关信号" : "直接信号"}
                        </span>
                        <span className="ml-auto shrink-0 tabular-nums text-[12px] text-q-text-muted">
                          {formatCompactTime(post.postedAt)}
                        </span>
                      </div>
                      <p className="mt-1 text-[14px] leading-relaxed text-q-text-primary" data-selectable="true">
                        {post.summary ?? post.translatedText ?? post.text}
                      </p>
                      <div className="mt-1 flex flex-wrap items-center gap-3">
                        <span className="text-[12px] tabular-nums text-q-text-muted">
                          北京时间 {radarBeijingTimeLabel(post.postedAt)}
                        </span>
                        {post.url ? (
                          <button
                            type="button"
                            className="inline-flex cursor-pointer items-center gap-1 text-[12px] text-q-primary hover:underline"
                            onClick={() => void openExternalUrl(post.url).catch(() => {})}
                          >
                            <ExternalLink size={12} aria-hidden />
                            查看原帖
                          </button>
                        ) : null}
                      </div>
                    </div>
                  ))}
                </div>
              ) : (
                <p className="text-[14px] leading-relaxed text-q-text-secondary">本轮没有引用重置相关帖子。</p>
              )}
            </div>
            <p className="text-[12px] text-q-text-muted">
              {aiReasoning.model ?? "未知模型"} · {formatTime(aiReasoning.createdAt)}
              {data?.analysisPrefs.rangeKey ? ` · ${formatRadarRangeLabel(data.analysisPrefs.rangeKey)}` : ""}
            </p>
            <button
              type="button"
              className="radar-inline-toggle self-start"
              data-open={analysisOpen || undefined}
              onClick={() => setAnalysisOpen((value) => !value)}
            >
              {analysisOpen ? "收起分析详情" : "查看分析详情"}
              <ChevronDown size={13} aria-hidden />
            </button>
            <AnimatedCollapse open={analysisOpen}>
              <div className="radar-collapse-scroll flex flex-col gap-2">
                <ListBlock title="支持依据" icon={<ThumbsUp size={13} aria-hidden />} items={aiReasoning.support.map((item) => humanizeRadarPostRefs(item, knownPosts))} />
                <ListBlock title="反向依据" icon={<ThumbsDown size={13} aria-hidden />} items={aiReasoning.against.map((item) => humanizeRadarPostRefs(item, knownPosts))} />
                <ListBlock title="不确定性" icon={<HelpCircle size={13} aria-hidden />} items={aiReasoning.uncertainty.map((item) => humanizeRadarPostRefs(item, knownPosts))} />
                <CitationList citations={aiReasoning.citations} posts={knownPosts} />
                <p className="text-[12px] text-q-text-muted">
                  {data && data.analysisPrefs.userPrompt.trim() && data.analysisPrefs.userPrompt !== data.analysisPrefs.defaultUserPrompt
                    ? "使用自定义语义提示"
                    : "使用默认语义提示"}
                </p>
              </div>
            </AnimatedCollapse>
          </>
        ) : (
          <>
            <p className="text-[14px] leading-relaxed text-q-text-secondary">
              {!ai || ai.state === "disabled"
                ? "AI 未启用：来源公告与本机验证不受影响。"
                : ai.state === "historical"
                  ? "没有针对当前范围的新分析；以下为最近一次历史结果。有新增动态时，下次检查会重新分析。"
                  : "尚未生成分析。可在 AI 辅助分析 Tab 开启后随立即检查运行。"}
            </p>
            {historicalAnalysis?.conclusion ? (
              <>
                <button
                  type="button"
                  className="radar-inline-toggle self-start"
                  data-open={historyAnalysisOpen || undefined}
                  onClick={() => setHistoryAnalysisOpen((value) => !value)}
                >
                  {historyAnalysisOpen ? "收起历史分析" : "查看历史分析"}
                  <ChevronDown size={13} aria-hidden />
                </button>
                <AnimatedCollapse open={historyAnalysisOpen}>
                  <div className="flex flex-col gap-1.5">
                    <p className="text-[13px] font-semibold text-q-primary">历史结论</p>
                    <p className="text-[14px] font-medium leading-relaxed text-q-text-secondary" data-selectable="true">
                      {humanizeRadarPostRefs(historicalAnalysis.conclusion, knownPosts)}
                    </p>
                    {historicalAnalysis.analysisBasis ? (
                      <>
                        <p className="text-[13px] font-semibold text-q-primary">历史分析</p>
                        <p className="text-[14px] leading-[1.65] text-q-text-secondary" data-selectable="true">
                          {humanizeRadarPostRefs(historicalAnalysis.analysisBasis, knownPosts)}
                        </p>
                      </>
                    ) : null}
                    <p className="text-[12px] text-q-text-muted">
                      {historicalAnalysis.model ?? "未知模型"} · {formatTime(historicalAnalysis.createdAt)}
                      {historicalAnalysis.rangeKey ? ` · ${formatRadarRangeLabel(historicalAnalysis.rangeKey)}` : ""}
                    </p>
                  </div>
                </AnimatedCollapse>
              </>
            ) : null}
          </>
        )}
      </section>

      <section className="glass-panel flex shrink-0 flex-col p-4">
        <button type="button" className="radar-collapse-trigger" onClick={() => setHistoryOpen((value) => !value)} aria-expanded={historyOpen}>
          <ChevronRight size={17} aria-hidden className={cn("radar-collapse-chevron", historyOpen && "is-open")} />
          <h2 className="text-[16px] font-semibold tracking-tight text-q-text-primary">检查历史</h2>
          <span className="min-w-0 flex-1 text-[12px] text-q-text-muted">共 {(data?.checks ?? []).length} 次检查</span>
        </button>
        <AnimatedCollapse open={historyOpen}>
          <div className="radar-collapse-scroll radar-history-scroll pt-2">
            {(data?.checks ?? []).length === 0 && <p className="text-xs text-q-text-muted">还没有检查记录</p>}
            <div className="flex flex-col">
              {(data?.checks ?? []).map((check) => (
                <div
                  key={check.id}
                  className="flex flex-wrap items-center gap-x-4 gap-y-1 border-b border-q-border/60 py-2 text-[13px] last:border-b-0"
                >
                  <span className="w-[104px] shrink-0 tabular-nums text-q-text-secondary">
                    {formatCompactTime(check.startedAt)}
                  </span>
                  <StatusBadge
                    tone={
                      check.status === "success"
                        ? "success"
                        : check.status === "running"
                          ? "primary"
                          : check.status === "partial"
                            ? "warning"
                            : "danger"
                    }
                  >
                    {check.status === "success"
                      ? "成功"
                      : check.status === "running"
                        ? "进行中"
                        : check.status === "partial"
                          ? "部分完成"
                          : "分析失败"}
                  </StatusBadge>
                  <span className="shrink-0 tabular-nums text-q-text-muted">同步 {check.postCount} 条</span>
                  {check.errorMessage && (
                    <span className="min-w-0 flex-1 truncate text-q-danger" title={check.errorMessage}>
                      {check.errorMessage}
                    </span>
                  )}
                </div>
              ))}
            </div>
          </div>
        </AnimatedCollapse>
      </section>

      <p className="flex shrink-0 items-center gap-1.5 px-1 text-[11px] text-q-text-muted">
        <ShieldCheck size={13} aria-hidden className="shrink-0" />
        雷达内容独立于平台额度状态；AI 分析只接收英文原文、发布时间与原帖链接，不发送凭据。
      </p>
      {confirmOpen ? (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/45 p-6 backdrop-blur-sm"
          role="presentation"
          onMouseDown={() => {
            if (!confirmReset.isPending) setConfirmOpen(false);
          }}
        >
          <div
            className="w-full max-w-sm rounded-[18px] border border-q-border bg-q-surface-solid p-5 shadow-q-lg"
            role="dialog"
            aria-modal="true"
            onMouseDown={(event) => event.stopPropagation()}
          >
            <p className="text-sm font-semibold text-q-text-primary">确认额度已经重置？</p>
            <p className="mt-2 text-xs leading-relaxed text-q-text-secondary">
              这只记录你的人工观察，不代表官方确认，也不会判断是官方重置还是使用了重置卡。
            </p>
            <div className="mt-4 flex justify-end gap-2">
              <Button variant="ghost" size="sm" disabled={confirmReset.isPending} onClick={() => setConfirmOpen(false)}>
                取消
              </Button>
              <Button
                size="sm"
                disabled={confirmReset.isPending}
                onClick={() =>
                  confirmReset.mutate(undefined, {
                    onSuccess: () => setConfirmOpen(false),
                  })
                }
              >
                {confirmReset.isPending ? "确认中…" : "确认"}
              </Button>
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}

/* ————————————————— Tibo 动态/* ————————————————— Tibo 动态（设计稿 05） ————————————————— */

function TiboFeedView({
  posts,
  rangeLabel,
  totalCount,
  signalCount,
  relatedCount,
  noneCount,
  selected,
  filter,
  onFilter,
  onSelect,
}: {
  posts: RadarPost[];
  rangeLabel: string;
  totalCount: number;
  signalCount: number;
  relatedCount: number;
  noneCount: number;
  selected: RadarPost | null;
  filter: "all" | "signal" | "related" | "none";
  onFilter: (value: "all" | "signal" | "related" | "none") => void;
  onSelect: (id: string) => void;
}) {
  const chips: Array<{ id: "all" | "signal" | "related" | "none"; label: string; count: number }> = [
    { id: "all", label: "全部", count: totalCount },
    { id: "signal", label: "直接信号", count: signalCount },
    { id: "related", label: "间接信号", count: relatedCount },
    { id: "none", label: "无关信号", count: noneCount },
  ];
  // 主从分栏由内容容器实际宽度决定（≥680 左右并排；不足时切列表/详情单面板，不上下堆叠）
  const container = useContainerWidth<HTMLDivElement>();
  const splitView = container.width >= TIBO_SPLIT_MIN_PX;
  const [pane, setPane] = useState<"list" | "detail">("list");
  // 详情滚动容器：选择新动态时回到顶部
  const detailRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (detailRef.current) detailRef.current.scrollTop = 0;
  }, [selected?.id]);
  // 紧凑单面板：display:none 会重置 scrollTop，切换前保存、返回后恢复列表滚动位置
  const listScrollRef = useRef<HTMLDivElement>(null);
  const listScrollTop = useRef(0);
  const goBackToList = () => {
    setPane("list");
    requestAnimationFrame(() => {
      if (listScrollRef.current) listScrollRef.current.scrollTop = listScrollTop.current;
    });
  };
  const pick = (id: string) => {
    onSelect(id);
    if (!splitView) {
      listScrollTop.current = listScrollRef.current?.scrollTop ?? 0;
      setPane("detail");
    }
  };

  return (
    <div ref={container.ref} className="flex min-h-0 flex-1 flex-col gap-3 overflow-hidden">
      {/* 筛选 chips：带计数（不随列表滚走） */}
      <p className="px-1 text-xs text-q-text-secondary">
        Tibo 动态 · {rangeLabel} · 共 {totalCount} 条
      </p>
      <div className="flex shrink-0 flex-wrap items-center gap-2 px-1">
        {chips.map((chip) => (
          <button
            key={chip.id}
            type="button"
            onClick={() => onFilter(chip.id)}
            aria-pressed={filter === chip.id}
            className={cn(
              "inline-flex cursor-pointer items-center gap-1.5 rounded-q-pill border px-3 py-1.5 text-[12px] font-medium transition-colors duration-150",
              filter === chip.id
                ? "border-[var(--q-chip-active-border)] bg-[var(--q-chip-active-bg)] text-[var(--q-chip-active-text)]"
                : "border-[var(--q-chip-border)] bg-[var(--q-chip-bg)] text-[var(--q-chip-text)] hover:border-[var(--q-chip-active-border)] hover:text-[var(--q-chip-active-text)]",
            )}
          >
            {chip.label}
            <span
              className={cn(
                "rounded-full px-1.5 text-[10px] tabular-nums",
                filter === chip.id
                  ? "bg-[var(--q-chip-count-bg)] text-[var(--q-chip-count-text)]"
                  : "bg-[var(--q-live-bg)] text-[var(--q-chip-text)]",
              )}
            >
              {chip.count}
            </span>
          </button>
        ))}
      </div>

      {/* 主从区域：高度由 Flex 分配，列表与详情各自独立滚动 */}
      <div
        className={cn(
          "min-h-0 flex-1 gap-4",
          splitView ? "grid grid-cols-[minmax(270px,0.85fr)_minmax(360px,1.4fr)]" : "flex",
        )}
      >
        {/* 动态列表：保持挂载（compact 切换详情时用 hidden），滚动位置与选中状态不丢 */}
        <section
          className={cn(
            "glass-panel flex min-h-0 min-w-0 flex-col gap-2.5 p-3.5",
            !splitView && pane !== "list" && "hidden",
            splitView && "min-w-0",
          )}
        >
          <div ref={listScrollRef} className="min-h-0 flex-1 space-y-2 overflow-y-auto pr-0.5">
            {posts.length === 0 && (
              <p className="px-1 py-6 text-center text-xs text-q-text-muted">点「立即检查」同步 CodexRadar。</p>
            )}
            {posts.map((post) => (
              <button
                key={post.id}
                type="button"
                onClick={() => pick(post.id)}
                aria-current={selected?.id === post.id ? "true" : undefined}
                className={cn(
                  "w-full cursor-pointer rounded-[14px] border p-3.5 text-left transition-colors duration-150",
                  selected?.id === post.id
                    ? "border-q-border-selected bg-q-primary-softer shadow-q-sm"
                    : "border-q-border bg-q-surface-strong hover:border-q-border-selected",
                )}
              >
                <div className="flex items-center gap-2">
                  <span title="来源分类">
                    <StatusBadge tone={postBadgeTone(post)}>{sourceRelationLabel(post)}</StatusBadge>
                  </span>
                  <span className="ml-auto shrink-0 text-[12px] tabular-nums text-q-text-secondary">
                    {formatTime(post.postedAt)}
                  </span>
                </div>
                <p className="mt-2 line-clamp-3 text-[13.5px] leading-[1.6] text-q-text-primary">
                  {post.summary ?? post.translatedText ?? post.text}
                </p>
                <div className="mt-2.5 flex items-center gap-3.5 text-[12px] text-q-text-secondary">
                  <span className="inline-flex items-center gap-1">
                    <MessageCircle size={12} aria-hidden />
                    {post.replies}
                  </span>
                  <span className="inline-flex items-center gap-1">
                    <Repeat2 size={12} aria-hidden />
                    {post.reposts}
                  </span>
                  <span className="inline-flex items-center gap-1">
                    <Heart size={12} aria-hidden />
                    {post.likes}
                  </span>
                </div>
              </button>
            ))}
          </div>
        </section>

        {/* 动态详情：独立滚动；英文原文随详情整体滚动，不再套第三层滚动框 */}
        <section
          ref={detailRef}
          className={cn(
            "glass-panel flex min-h-0 min-w-0 flex-col gap-3.5 overflow-y-auto p-4",
            !splitView && pane !== "detail" && "hidden",
          )}
        >
          {!splitView && selected && (
            <button
              type="button"
              onClick={goBackToList}
              className="flex w-fit cursor-pointer items-center gap-1.5 self-start rounded-q-pill border border-[var(--q-chip-border)] bg-[var(--q-chip-bg)] px-3 py-1.5 text-[12px] font-medium text-[var(--q-chip-text)] transition-colors hover:border-[var(--q-chip-active-border)] hover:text-[var(--q-chip-active-text)]"
            >
              <ArrowLeft size={13} aria-hidden />
              返回动态列表
            </button>
          )}
          {selected ? (
            <>
              <div className="flex flex-wrap items-center gap-2.5">
                <span title="来源分类">
                  <StatusBadge tone={postBadgeTone(selected)}>{sourceRelationLabel(selected)}</StatusBadge>
                </span>
                <span className="text-[11px] tabular-nums text-q-text-muted">{formatTime(selected.postedAt)}</span>
              </div>

              <div className="flex flex-col gap-2">
                <p className="text-xs font-medium text-q-text-secondary">中文翻译</p>
                <div
                  className="rounded-q-control border border-q-border bg-q-surface-strong px-3.5 py-3 text-[13px] leading-relaxed text-q-text-primary"
                  data-selectable="true"
                >
                  {selected.translatedText ?? selected.summary ?? "尚无中文翻译"}
                </div>
              </div>

              <div className="flex flex-col gap-2">
                <p className="text-xs font-medium text-q-text-secondary">英文原文</p>
                <div
                  className="whitespace-pre-wrap rounded-q-control border border-q-border bg-q-surface-muted px-3.5 py-3 text-[13px] leading-relaxed text-q-text-secondary"
                  data-selectable="true"
                >
                  {selected.text}
                </div>
              </div>

              {selected.analysis ? (
                <div className="flex flex-col gap-2">
                  <p className="text-xs font-medium text-q-text-secondary">CodexRadar 解读 · 仅为上游参考</p>
                  <div className="rounded-q-control border border-q-border bg-q-primary-softer px-3.5 py-3 text-[13px] leading-relaxed text-q-text-primary">
                    {selected.analysis}
                  </div>
                </div>
              ) : null}

              <div className="mt-2 flex flex-wrap gap-2 pt-1">
                {selected.url && (
                  <Button size="sm" onClick={() => void openExternalUrl(selected.url)}>
                    <ExternalLink size={14} aria-hidden />
                    打开 X 原帖
                  </Button>
                )}
                <Button variant="secondary" size="sm" onClick={() => void openExternalUrl("https://codexradar.com/")}>
                  <Radar size={14} aria-hidden />
                  CodexRadar 来源
                </Button>
              </div>
            </>
          ) : (
            <p className="text-xs text-q-text-muted">
              {splitView ? "从左侧选择一条动态" : "从动态列表选择一条动态"}
            </p>
          )}
        </section>
      </div>
    </div>
  );
}

/* ————————————————— AI 辅助分析（设计稿 06） ————————————————— */

function AiAnalysisView({
  data,
  analyze,
  sourceId,
  modelChoice,
  models,
  userPrompt,
  defaultUserPrompt,
  onAnalyzeChange,
  onSourceChange,
  onModelChange,
  onUserPromptChange,
}: {
  data: Awaited<ReturnType<typeof fetchRadarSnapshot>> | undefined;
  analyze: boolean;
  sourceId: string;
  modelChoice: string;
  models: RadarModelOption[];
  userPrompt: string;
  defaultUserPrompt: string;
  onAnalyzeChange: (value: boolean) => void;
  onSourceChange: (value: string) => void;
  onModelChange: (value: string) => void;
  onUserPromptChange: (value: string) => void;
}) {
  const analysis = data?.analysis;
  const knownPosts = data?.posts ?? [];
  const latestCheck = data?.checks[0];
  const analyzeError = latestCheck?.analyzeStatus === "failed" ? latestCheck.errorMessage : null;
  const byId = useMemo(() => new Map(knownPosts.map((post) => [post.id, post])), [knownPosts]);
  const pickPosts = (ids: string[]) => ids.map((id) => byId.get(id)).filter((post): post is RadarPost => Boolean(post));
  const analysisInput = pickPosts(data?.analysisGroups.newPostIds ?? []);
  const contextInput = pickPosts(data?.analysisGroups.eventContextIds ?? []);
  const historicalInput = pickPosts(data?.analysisGroups.historicalContextIds ?? []);
  // 所选来源 + 模型的选项；modelChoice 失配（来源切换、旧偏好）时回退该来源默认模型。
  const selectedModelOption =
    models.find((item) => item.sourceId === sourceId && item.model === modelChoice) ??
    models.find((item) => item.sourceId === sourceId) ??
    null;
  const selectClass =
    "h-10 w-full cursor-pointer rounded-q-control border border-q-border bg-q-surface-strong px-3 text-sm text-q-text-primary outline-none focus:border-q-primary";

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto pr-1">
      <div className="grid grid-cols-[repeat(auto-fit,minmax(min(100%,320px),1fr))] gap-4">
        {/* 分析配置 */}
        <section className="glass-panel flex flex-col gap-4 p-4">
          <h2 className="text-[14px] font-semibold tracking-tight text-q-text-primary">分析配置</h2>
          <div className="flex items-center justify-between gap-4">
            <div className="min-w-0">
              <p className="text-sm font-medium text-q-text-primary">立即检查时同时运行 AI 分析</p>
              <p className="mt-0.5 text-xs leading-relaxed text-q-text-muted">默认关闭；关闭后仅同步与展示来源内容。</p>
            </div>
            <Switch checked={analyze} onCheckedChange={onAnalyzeChange} label="立即检查时同时运行 AI 分析" />
          </div>
          <p className="text-xs leading-relaxed text-q-text-muted">
            分析范围使用雷达顶部的「动态范围」，AI 关闭时也可以调整。
          </p>
          <label className="flex flex-col gap-1.5 text-sm">
            <span className="font-medium text-q-text-primary">分析模型</span>
            <select
              value={
                selectedModelOption ? `${selectedModelOption.sourceId}|${selectedModelOption.model}` : ""
              }
              onChange={(event) => {
                const [nextSource, ...rest] = event.target.value.split("|");
                onSourceChange(nextSource);
                onModelChange(rest.join("|"));
              }}
              className={selectClass}
            >
              {models.length === 0 && <option value="">请先在平台中心接入 API Key</option>}
              {models.map((item) => (
                <option
                  key={`${item.sourceId}|${item.model}`}
                  value={`${item.sourceId}|${item.model}`}
                  disabled={!item.ready}
                >
                  {item.model || "仅自定义模型（在下方添加）"}
                  {item.ready ? "" : "（不可用）"}
                </option>
              ))}
            </select>
          </label>
          <CustomModelPanel models={models} selected={selectedModelOption} onAdded={onModelChange} />
          <p className="text-xs leading-relaxed text-q-text-muted">
            开启 AI 时只分析所选范围内尚未消费的新帖；关闭时仍同步来源，不调用模型，未消费帖子保持待分析。
          </p>
        </section>

        {/* 本次输入 */}
        <section className="glass-panel flex flex-col gap-3 p-4">
          <h2 className="text-[14px] font-semibold tracking-tight text-q-text-primary">本次输入</h2>
          <div className="flex items-start gap-2.5 rounded-q-control border border-q-border bg-q-primary-softer px-3 py-2.5">
            <ShieldCheck size={15} aria-hidden className="mt-0.5 shrink-0 text-q-primary" />
            <p className="text-xs leading-relaxed text-q-text-secondary">
              只发送英文原文、时间和原帖链接。不发送 CodexRadar 的中文翻译、信号标签或模型语境解读。
            </p>
          </div>
          {analysisInput.length === 0 && contextInput.length === 0 && historicalInput.length === 0 ? (
            <p className="text-xs leading-relaxed text-q-text-muted">
              当前时间窗内没有可分析的帖子。Tibo 近期没有新动态时，可切换「自定义」扩大时间范围后重试。
            </p>
          ) : (
            <div className="flex min-h-0 flex-col gap-3">
              <PostGroupPreview title="本次新增帖子" subtitle="真正未消费，才能创建或推进事件" posts={analysisInput} />
              {contextInput.length > 0 && (
                <PostGroupPreview title="事件上下文" subtitle="已关联当前事件，不能单独作为新证据" posts={contextInput} />
              )}
              {historicalInput.length > 0 && (
                <PostGroupPreview title="历史上下文" subtitle="已消费或已关闭事件帖，只能生成历史解释" posts={historicalInput} />
              )}
            </div>
          )}
        </section>
      </div>

      {/* 语义提示 */}
      <section className="glass-panel flex flex-col gap-2.5 p-4">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <h2 className="text-[14px] font-semibold tracking-tight text-q-text-primary">语义提示</h2>
          <button
            type="button"
            className="cursor-pointer text-xs font-medium text-q-primary hover:underline"
            onClick={() => onUserPromptChange(defaultUserPrompt)}
            disabled={!defaultUserPrompt || userPrompt === defaultUserPrompt}
          >
            恢复默认
          </button>
        </div>
        <p className="text-xs leading-relaxed text-q-text-muted">
          会随每次分析发给模型，用来补充你认为算强重置信号的措辞。不会改变只发送英文原文、时间和链接的限制。留空则不做额外语义引导。
        </p>
        <textarea
          value={userPrompt}
          onChange={(event) => onUserPromptChange(event.target.value.slice(0, 4000))}
          rows={6}
          maxLength={4000}
          placeholder="例如：提到 dashboard、milestone、Hold on to your Codex 时视为即将重置的强信号。"
          className="min-h-[132px] w-full resize-y rounded-q-control border border-q-border bg-q-surface-strong px-3 py-2.5 text-[13px] leading-relaxed text-q-text-primary outline-none focus:border-q-primary"
        />
        <p className="text-[11px] text-q-text-muted">{userPrompt.length}/4000</p>
      </section>

      {/* 辅助结论 */}
      <section className="glass-panel flex flex-col gap-3 p-4">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <h2 className="text-[14px] font-semibold tracking-tight text-q-text-primary">最近成功分析</h2>
          <div className="flex flex-wrap items-center gap-2">
            {analysis?.model ? <span className="text-[11px] text-q-text-muted">{analysis.model}</span> : null}
            {analysis ? (
              <span className="text-[11px] text-q-text-muted">{formatTime(analysis.createdAt)} 分析</span>
            ) : null}
          </div>
        </div>
        {analyzeError ? <p className="text-xs leading-relaxed text-q-danger">{analyzeError}</p> : null}
        {data?.aiAssessment.state === "pending" ? (
          <p className="text-xs text-q-warning">有新动态待分析，下次「立即检查」时更新。</p>
        ) : null}
        {data?.aiAssessment.state === "historical" || (!data?.aiAssessment.enabled && analysis) ? (
          <p className="text-xs text-q-text-muted">以下为历史分析，不冒充最新结论。</p>
        ) : null}
        {data?.analysisGroups.mode === "historical_replay" ? (
          <p className="text-xs text-q-text-muted">当前范围为历史回放：可以生成解释，不会创建或推进事件。</p>
        ) : null}
        {analysis?.conclusion ? (
          <>
            <div className="flex flex-col gap-1.5">
              <p className="text-[11px] font-semibold tracking-widest text-q-primary">结论</p>
              <p className="text-[15px] font-medium leading-relaxed text-q-text-primary" data-selectable="true">
                {humanizeRadarPostRefs(analysis.conclusion, knownPosts)}
              </p>
            </div>
            {analysis.analysisBasis ? (
              <div className="flex flex-col gap-1.5">
                <p className="text-[11px] font-semibold tracking-widest text-q-primary">
                  分析依据 · {humanizeRadarPostRefs(analysis.analysisBasis, knownPosts).length} 字
                </p>
                <div
                  className="max-h-72 overflow-y-auto rounded-q-card border border-q-border bg-q-primary-softer px-3.5 py-3 text-[13px] leading-relaxed text-q-text-secondary"
                  data-selectable="true"
                >
                  {humanizeRadarPostRefs(analysis.analysisBasis, knownPosts)}
                </div>
              </div>
            ) : null}
            <div className="grid grid-cols-[repeat(auto-fit,minmax(min(100%,320px),1fr))] gap-x-8 gap-y-3">
              <CitationList citations={analysis.citations} posts={knownPosts} />
              <ListBlock title="支持依据" icon={<ThumbsUp size={13} aria-hidden />} items={analysis.support.map((item) => humanizeRadarPostRefs(item, knownPosts))} />
              <ListBlock title="反向依据" icon={<ThumbsDown size={13} aria-hidden />} items={analysis.against.map((item) => humanizeRadarPostRefs(item, knownPosts))} />
              <ListBlock title="不确定性" icon={<HelpCircle size={13} aria-hidden />} items={analysis.uncertainty.map((item) => humanizeRadarPostRefs(item, knownPosts))} />
            </div>
          </>
        ) : (
          <p className="text-xs text-q-text-muted">
            {analyzeError ? "分析未完成，请更换模型或稍后重试。" : "尚未生成分析。开启 AI 后点立即检查。"}
          </p>
        )}
      </section>

      <p className="flex items-center gap-1.5 px-1 text-[11px] text-q-text-muted">
        <BrainCircuit size={13} aria-hidden className="shrink-0" />
        AI 是解释器而不是证据来源：每个输出必须引用实际原文；没有真实原文或分析失败时不生成结论。
      </p>
    </div>
  );
}

/* ——— 自定义分析模型：输入模型名 → 验证连接 → 保存入库 ——— */

function CustomModelPanel({
  models,
  selected,
  onAdded,
}: {
  models: RadarModelOption[];
  selected: RadarModelOption | null;
  onAdded: (model: string) => void;
}) {
  const queryClient = useQueryClient();
  const [open, setOpen] = useState(false);
  const [draft, setDraft] = useState("");
  const [verifiedModel, setVerifiedModel] = useState<string | null>(null);
  const [feedback, setFeedback] = useState<{ kind: "ok" | "error"; text: string } | null>(null);

  const trimmed = draft.trim();
  const sourceReady = Boolean(selected?.ready);

  const testMutation = useMutation({
    mutationFn: () => {
      if (!selected) return Promise.reject(new Error("请先选择一个分析模型来源"));
      return testRadarModel({ sourceId: selected.sourceId, model: trimmed });
    },
    onSuccess: () => {
      setVerifiedModel(trimmed);
      setFeedback({ kind: "ok", text: "连接成功，可以保存。" });
    },
    onError: (error) => {
      setVerifiedModel(null);
      setFeedback({ kind: "error", text: ipcErrorMessage(error, "操作失败") });
    },
  });
  const addMutation = useMutation({
    mutationFn: (model: string) => {
      if (!selected) return Promise.reject(new Error("请先选择一个分析模型来源"));
      return addRadarCustomModel({ sourceId: selected.sourceId, model });
    },
    onSuccess: (snapshot, model) => {
      queryClient.setQueryData(RADAR_SNAPSHOT_QUERY_KEY, snapshot);
      onAdded(model);
      setDraft("");
      setVerifiedModel(null);
      setFeedback({ kind: "ok", text: `已保存 ${model}，已加入上方下拉。` });
    },
    onError: (error) => setFeedback({ kind: "error", text: ipcErrorMessage(error, "操作失败") }),
  });
  const deleteMutation = useMutation({
    mutationFn: (model: string) => {
      if (!selected) return Promise.reject(new Error("请先选择一个分析模型来源"));
      return deleteRadarCustomModel({ sourceId: selected.sourceId, model });
    },
    onSuccess: (snapshot) => queryClient.setQueryData(RADAR_SNAPSHOT_QUERY_KEY, snapshot),
    onError: (error) => setFeedback({ kind: "error", text: ipcErrorMessage(error, "操作失败") }),
  });

  const changeDraft = (value: string) => {
    setDraft(value);
    setVerifiedModel(null);
    setFeedback(null);
  };

  // 切换来源后，旧来源的验证结果不再有效，避免未验证就保存到新来源。
  useEffect(() => {
    setVerifiedModel(null);
    setFeedback(null);
  }, [selected?.sourceId]);

  const customOptions = selected
    ? models.filter((item) => item.custom && item.sourceId === selected.sourceId)
    : [];

  return (
    <div className="flex flex-col gap-2">
      <button
        type="button"
        className="self-start cursor-pointer text-xs font-medium text-q-primary hover:underline"
        onClick={() => setOpen((value) => !value)}
      >
        {open ? "收起自定义模型" : "添加自定义模型"}
      </button>
      {open ? (
        selected ? (
          <div className="flex flex-col gap-2 rounded-q-control border border-q-border bg-q-surface px-3 py-2.5">
            <p className="text-xs leading-relaxed text-q-text-muted">
              添加平台支持的任意模型名：先验证连接（发送一次极小请求），通过后保存即可加入上方下拉。
            </p>
            <div className="flex flex-wrap items-center gap-2">
              <input
                value={draft}
                onChange={(event) => changeDraft(event.target.value)}
                placeholder="模型名称，例如 deepseek-reasoner"
                disabled={!sourceReady}
                className="h-9 min-w-0 flex-1 rounded-md border border-q-border bg-q-surface px-2.5 text-xs text-q-text-primary outline-none focus:border-q-primary disabled:opacity-50"
              />
              <Button
                variant="secondary"
                size="sm"
                disabled={!sourceReady || !trimmed || testMutation.isPending}
                onClick={() => testMutation.mutate()}
              >
                {testMutation.isPending ? "验证中…" : verifiedModel === trimmed ? "重新验证" : "验证连接"}
              </Button>
              <Button
                size="sm"
                disabled={!sourceReady || !trimmed || verifiedModel !== trimmed || addMutation.isPending}
                onClick={() => addMutation.mutate(trimmed)}
              >
                {addMutation.isPending ? "保存中…" : "保存"}
              </Button>
            </div>
            {!sourceReady ? (
              <p className="text-xs text-q-text-muted">当前来源凭据不可用，先在平台中心修复后再添加模型。</p>
            ) : null}
            {feedback ? (
              <p className={cn("text-xs leading-relaxed", feedback.kind === "ok" ? "text-q-success" : "text-q-danger")}>
                {feedback.text}
              </p>
            ) : null}
            {customOptions.length > 0 ? (
              <div className="flex flex-wrap items-center gap-1.5">
                {customOptions.map((item) => (
                  <span
                    key={item.model}
                    className="inline-flex items-center gap-1 rounded-q-pill border border-q-border bg-q-surface-strong py-0.5 pl-2.5 pr-1 text-[11px] text-q-text-secondary"
                  >
                    {item.model}
                    <button
                      type="button"
                      aria-label={`删除自定义模型 ${item.model}`}
                      disabled={deleteMutation.isPending}
                      onClick={() => deleteMutation.mutate(item.model)}
                      className="cursor-pointer rounded-full px-1 text-q-text-muted hover:bg-q-danger-soft hover:text-q-danger disabled:opacity-50"
                    >
                      ×
                    </button>
                  </span>
                ))}
              </div>
            ) : null}
          </div>
        ) : (
          <p className="text-xs text-q-text-muted">请先在上方下拉中选择一个分析模型来源。</p>
        )
      ) : null}
    </div>
  );
}

function PostGroupPreview({
  title,
  subtitle,
  posts,
}: {
  title: string;
  subtitle: string;
  posts: RadarPost[];
}) {
  return (
    <div className="flex min-h-0 flex-col gap-2">
      <p className="text-xs text-q-text-secondary">
        {title} · {posts.length} 条
        <span className="ml-1.5 text-[11px] text-q-text-muted">{subtitle}</span>
      </p>
      <div className="max-h-52 space-y-1.5 overflow-y-auto pr-0.5">
        {posts.slice(0, 8).map((post) => (
          <div
            key={post.id}
            className="rounded-q-control border border-q-border bg-q-surface-strong px-3 py-2 text-[12px] leading-relaxed"
          >
            <span className="mr-2 shrink-0 tabular-nums text-q-text-muted">{formatTime(post.postedAt)}</span>
            <span className="text-q-text-primary">{post.text}</span>
          </div>
        ))}
        {posts.length > 8 && <p className="px-1 text-[11px] text-q-text-muted">…共 {posts.length} 条</p>}
      </div>
    </div>
  );
}

function ListBlock({
  title,
  icon,
  items,
}: {
  title: string;
  icon: React.ReactNode;
  items: string[];
}) {
  if (items.length === 0) return null;
  return (
    <div className="flex flex-col gap-1.5">
      <p className="flex items-center gap-1.5 text-xs font-medium text-q-text-secondary">
        <span aria-hidden className="text-q-primary">
          {icon}
        </span>
        {title}
      </p>
      <ul className="flex flex-col gap-1 pl-0.5">
        {items.map((item, index) => (
          <li
            key={`${index}-${item.slice(0, 12)}`}
            className="flex gap-2 rounded-q-control border border-q-border bg-q-surface-strong px-3 py-2 text-xs leading-relaxed text-q-text-primary"
            data-selectable="true"
          >
            <span className="shrink-0 tabular-nums font-medium text-q-primary">{index + 1}.</span>
            <span className="min-w-0 break-words">{item}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}

/** 引用卡：别名映射回真实 post_id 后，展示帖子原文 + 时间 + 查看原帖，不暴露原始帖子编号。 */
function CitationList({ citations, posts, compact = false }: { citations: string[]; posts: RadarPost[]; compact?: boolean }) {
  const [linkError, setLinkError] = useState<string | null>(null);
  if (citations.length === 0) return null;
  const byId = new Map(posts.map((post) => [post.id, post]));
  return (
    <div className="flex flex-col gap-1.5">
      <p className="flex items-center gap-1.5 text-xs font-medium text-q-text-secondary">
        <span aria-hidden className="text-q-primary">
          <Link2 size={13} aria-hidden />
        </span>
        引用
        <span className="text-q-text-muted">· {citations.length}</span>
      </p>
      <ul className={cn("gap-1.5 pl-0.5", compact ? "flex flex-wrap" : "flex flex-col")}>
        {citations.map((citation, index) => {
          const post = byId.get(citation);
          return (
            <li
              key={`${index}-${citation.slice(0, 12)}`}
              className={cn(
                "flex items-center gap-2 border border-q-border bg-q-surface-strong text-xs leading-relaxed text-q-text-primary",
                compact ? "rounded-q-pill px-2.5 py-1" : "flex-col items-stretch rounded-q-control px-3 py-2",
              )}
            >
              {!compact && post ? (
                <>
                  <div className="flex items-center gap-2">
                    <span className="shrink-0 tabular-nums font-medium text-q-primary">{index + 1}.</span>
                    <span className="shrink-0 tabular-nums text-q-text-muted">{citationTimeLabel(post.postedAt)}</span>
                    <button
                      type="button"
                      className="ml-auto inline-flex shrink-0 cursor-pointer items-center gap-1 text-q-primary hover:underline"
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
                  </div>
                  <p className="min-w-0 text-[13px] leading-relaxed text-q-text-secondary" data-selectable="true">
                    {post.summary ?? post.translatedText ?? post.text}
                  </p>
                </>
              ) : (
                <>
                  {!compact ? <span className="shrink-0 tabular-nums font-medium text-q-primary">{index + 1}.</span> : null}
                  {post ? (
                    <>
                      <span className="min-w-0 shrink-0 tabular-nums text-q-text-muted">{citationTimeLabel(post.postedAt)}</span>
                      <button
                        type="button"
                        className="inline-flex cursor-pointer items-center gap-1 text-q-primary hover:underline"
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
                    </>
                  ) : (
                    <span className="min-w-0 text-q-text-muted">引用帖（不在当前同步列表）</span>
                  )}
                </>
              )}
            </li>
          );
        })}
      </ul>
      {linkError ? <p className="text-[11px] text-q-danger">{linkError}</p> : null}
    </div>
  );
}

/** 引用时间标签：固定 `MM-DD HH:mm`。 */
function citationTimeLabel(epochMs: number): string {
  const date = new Date(epochMs);
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

/** 相对天数档：`Nd` 表示过去 N 天（与后端 parse_range_days 对齐）。 */
function parseRangeDays(rangeKey: string): number | null {
  if (!rangeKey.endsWith("d")) return null;
  const days = Number(rangeKey.slice(0, -1));
  return Number.isInteger(days) && days >= 1 && days <= 365 ? days : null;
}

/** 自定义日期区间档：`range:YYYY-MM-DD:YYYY-MM-DD`。 */
function parseCustomRange(rangeKey: string): { start: string; end: string } | null {
  const match = /^range:(\d{4}-\d{2}-\d{2}):(\d{4}-\d{2}-\d{2})$/.exec(rangeKey);
  return match ? { start: match[1], end: match[2] } : null;
}

function toIsoDate(date: Date): string {
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${date.getFullYear()}-${month}-${day}`;
}

/** 默认自定义区间：最近 30 天。 */
function defaultCustomRangeKey(): string {
  const today = new Date();
  const start = new Date(today.getTime() - 30 * 24 * 60 * 60 * 1000);
  return `range:${toIsoDate(start)}:${toIsoDate(today)}`;
}

/** 自定义档的当前值：日期区间优先；旧的 Nd 偏好换算成区间回显。 */
function customRangeOf(rangeKey: string): { start: string; end: string } | null {
  const parsed = parseCustomRange(rangeKey);
  if (parsed) return parsed;
  const days = parseRangeDays(rangeKey);
  if (days === null) return null;
  const today = new Date();
  const start = new Date(today.getTime() - days * 24 * 60 * 60 * 1000);
  return { start: toIsoDate(start), end: toIsoDate(today) };
}

/** 时间范围快捷档（与 Tibo 筛选 chips 同款视觉）。 */
const QUICK_RANGES: Array<{ id: string; label: string }> = [
  { id: "today", label: "当天" },
  { id: "3d", label: "过去 3 天" },
  { id: "7d", label: "过去 7 天" },
];

/** 时间范围快捷档与筛选 Chip 的统一样式：两主题各由 Chip Token 驱动，不再写死白底。 */
function rangeChipClass(active: boolean): string {
  return cn(
    "inline-flex cursor-pointer items-center rounded-q-pill border px-3 py-1.5 text-[12px] font-medium transition-colors duration-150",
    active
      ? "border-[var(--q-chip-active-border)] bg-[var(--q-chip-active-bg)] text-[var(--q-chip-active-text)]"
      : "border-[var(--q-chip-border)] bg-[var(--q-chip-bg)] text-[var(--q-chip-text)] hover:border-[var(--q-chip-active-border)] hover:text-[var(--q-chip-active-text)]",
  );
}
