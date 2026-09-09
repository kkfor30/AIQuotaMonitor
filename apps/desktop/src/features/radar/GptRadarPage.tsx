import { type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Calendar,
  ChevronDown,
  ChevronRight,
  CreditCard,
  ExternalLink,
  Heart,
  HelpCircle,
  History,
  MessageCircle,
  Radar,
  RefreshCw,
  Repeat2,
  RotateCcw,
  Settings2,
  ShieldCheck,
  Sparkles,
  ThumbsDown,
  ThumbsUp,
  X,
} from "lucide-react";
import { Button } from "@/components/ui/Button";
import { Switch } from "@/components/ui/Switch";
import { StatusBadge } from "@/components/ui/StatusBadge";
import { RadarSkeleton } from "@/components/ui/PageSkeletons";
import { cn } from "@/lib/cn";
import {
  addRadarCustomModel,
  cancelRadarCheck,
  confirmRadarQuotaChange,
  confirmRadarUserReset,
  deleteRadarChatEndpoint,
  deleteRadarCustomModel,
  fetchRadarSnapshot,
  ipcErrorMessage,
  listRadarHistory,
  listRadarEventAnalyses,
  listRadarPosts,
  openExternalUrl,
  runRadarCheck,
  saveRadarAnalysisPrefs,
  saveRadarChatEndpoint,
  setRadarNoticeHidden,
  testRadarChatEndpoint,
  testRadarModel,
  translateRadarPost,
  undoRadarUserReset,
} from "@/lib/ipc";
import { RADAR_SNAPSHOT_QUERY_KEY } from "@/lib/query-client";
import type {
  RadarChatEndpoint,
  RadarHistory,
  RadarHistoryEvent,
  RadarModelOption,
  RadarPost,
  RadarSnapshot,
} from "@/lib/ipc";
import {
  formatRadarRangeLabel,
  humanizeRadarPostRefs,
  quotaBadgeLabel,
  quotaCorrelationLabel,
  radarAccountBankedChangeNotes,
  radarAiStatusLabel,
  radarBankedChangeMeta,
  radarBankedChangeRows,
  radarBankedChangeTitle,
  radarBeijingTimeLabel,
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
} from "@/features/hoverbar/hoverbar-state";
import { useContainerWidth, TIBO_SPLIT_MIN_PX } from "@/lib/use-container-width";
import { ArrowLeft } from "lucide-react";

export type TiboFilter = "all" | "signal" | "related" | "unknown" | "none";

export function GptRadarPage() {
  const queryClient = useQueryClient();
  const [tab, setTab] = useState<RadarTabId>("signal");
  const [filter, setFilter] = useState<TiboFilter>("all");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  // 浏览筛选：只影响 Tibo 列表展示，不触发分析、不改变监控窗口与当前判断。
  const [browseRange, setBrowseRange] = useState<string>("all");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const settingsButtonRef = useRef<HTMLButtonElement>(null);
  const [extraPosts, setExtraPosts] = useState<RadarPost[]>([]);
  const [postsLoadingMore, setPostsLoadingMore] = useState(false);
  const [postsExhausted, setPostsExhausted] = useState(false);

  const { data, isLoading } = useQuery({
    queryKey: RADAR_SNAPSHOT_QUERY_KEY,
    queryFn: fetchRadarSnapshot,
  });
  // 跨窗口检查状态：悬浮窗/后台触发检查时主窗口按钮同步 loading。
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

  // 检查使用已保存的偏好设置；修改设置在「雷达设置」抽屉显式保存，刷新不偷偷保存。
  const modelOptions = (data?.models ?? []).filter((item) => item.ready);
  const chosenModel =
    modelOptions.find(
      (item) =>
        item.sourceId === data?.analysisPrefs.sourceId && item.model === data?.analysisPrefs.model,
    ) ??
    modelOptions.find((item) => item.sourceId === data?.analysisPrefs.sourceId && item.model) ??
    modelOptions.find((item) => item.model) ??
    modelOptions[0] ??
    null;

  const checkMutation = useMutation({
    mutationFn: () =>
      runRadarCheck({
        analyze: data?.analysisPrefs.analyze ?? false,
        sourceId: chosenModel?.sourceId ?? null,
        model: chosenModel?.model ?? null,
      }),
    onSuccess: (snapshot) => {
      queryClient.setQueryData(RADAR_SNAPSHOT_QUERY_KEY, snapshot);
    },
  });

  const translateMutation = useMutation({
    mutationFn: (postId: string) => translateRadarPost(postId, chosenModel?.sourceId ?? null),
    onSuccess: (snapshot) => {
      queryClient.setQueryData(RADAR_SNAPSHOT_QUERY_KEY, snapshot);
    },
  });

  const radarChecking = checkMutation.isPending || externalChecking || Boolean(data?.checkRunning);
  const checkCancelled = checkMutation.error
    ? ipcErrorMessage(checkMutation.error, "检查失败").includes("已终止")
    : false;
  // Tibo 动态是浏览视图：展示同步到的全部帖子，基于全局唯一 ID 严格去重；时间筛选仅作用于展示。
  useEffect(() => {
    setExtraPosts([]);
    setPostsExhausted(false);
  }, [data?.lastSyncedAt]);
  const rawPosts = [...(data?.posts ?? []), ...extraPosts];
  const seenIds = new Set<string>();
  const posts: RadarPost[] = [];
  for (const post of rawPosts) {
    if (!seenIds.has(post.id)) {
      seenIds.add(post.id);
      posts.push(post);
    }
  }
  posts.sort((a, b) => b.postedAt - a.postedAt);
  const browseBounds = browseRangeBounds(browseRange);
  const browsed = posts.filter((post) =>
    browseRange === "all"
      ? true
      : post.postedAt >= browseBounds.start && post.postedAt <= browseBounds.end,
  );
  const visible = browsed.filter((post) => {
    if (filter === "all") return true;
    if (filter === "signal") return post.explicitReset || post.filter === "signal";
    if (filter === "related") return post.filter === "related";
    if (filter === "unknown") return post.filter === "unknown" && !post.explicitReset;
    return post.filter === "none" && !post.explicitReset;
  });
  const selected = visible.find((post) => post.id === selectedId) ?? visible[0] ?? null;

  const customRange = customRangeOf(browseRange);
  const customActive =
    browseRange !== "all" &&
    !(["today", "3d", "7d"] as string[]).includes(browseRange) &&
    customRange !== null;
  const todayIso = toIsoDate(new Date());
  const [customPickerOpen, setCustomPickerOpen] = useState(false);
  const customPickerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!customPickerOpen) return;
    const handleClickOutside = (e: MouseEvent) => {
      if (customPickerRef.current && !customPickerRef.current.contains(e.target as Node)) {
        setCustomPickerOpen(false);
      }
    };
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, [customPickerOpen]);

  // 记住最近一次自定义区间：切到快捷档再切回「自定义」时恢复，而不是重置成默认 30 天
  const lastCustomRangeRef = useRef<string | null>(
    customRange ? `range:${customRange.start}:${customRange.end}` : null,
  );
  const applyCustomRange = (start: string, end: string) => {
    if (!start || !end || start.length !== 10 || end.length !== 10) return;
    const [from, to] = start <= end ? [start, end] : [end, start];
    lastCustomRangeRef.current = `range:${from}:${to}`;
    setBrowseRange(`range:${from}:${to}`);
  };
  const switchToCustom = () => {
    setBrowseRange(lastCustomRangeRef.current ?? defaultCustomRangeKey());
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-hidden p-4 pt-2 pr-2 animate-fade-in">
      <header className="glass-panel flex shrink-0 items-center gap-x-2.5 gap-y-0.5 px-4 py-2.5">
        <span
          aria-hidden
          className="flex h-8 w-8 shrink-0 items-center justify-center rounded-[10px] border border-q-border bg-q-surface-strong text-q-primary shadow-q-sm"
        >
          <Radar size={16} aria-hidden />
        </span>
        <h1 className="shrink-0 text-[17px] font-semibold tracking-tight text-q-text-primary">GPT 重置雷达</h1>
        <span className="shrink-0 whitespace-nowrap rounded-q-pill bg-q-neutral-soft px-2 py-0.5 text-[11px] text-q-neutral">
          仅为推测，不代表官方结论
        </span>
        <SourceStatusChips data={data} className="ml-auto hidden md:flex" />
      </header>
      {checkMutation.error && !checkCancelled && (
        <p className="rounded-q-control border border-q-danger/25 bg-q-danger-soft px-3 py-2 text-xs text-q-danger">
          {ipcErrorMessage(checkMutation.error, "检查失败")}
        </p>
      )}

      {/* 分段 Tab + 立即检查 + 雷达设置（时间筛选只在「全部动态」页内且仅作用于浏览） */}
      <div className="flex shrink-0 flex-wrap items-center justify-between gap-x-4 gap-y-2">
        <div className="flex shrink-0 items-center gap-3">
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
        <Button
          ref={settingsButtonRef}
          variant="secondary"
          size="sm"
          className="ml-auto mr-4 shrink-0"
          onClick={() => setSettingsOpen(true)}
        >
          <Settings2 size={14} aria-hidden />
          雷达设置
        </Button>
      </div>

      {isLoading ? (
        <RadarSkeleton />
      ) : (
        <>
          {tab === "signal" && <SignalSummaryView data={data} />}
          {tab === "tibo" && (
            <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-hidden">
              {/* 浏览时间筛选：仅过滤本地已同步数据，不触发分析、不改变监控范围 */}
              <div className="flex shrink-0 flex-wrap items-center gap-2 px-1">
                <span className="shrink-0 whitespace-nowrap text-[12.5px] font-medium text-q-text-secondary">
                  浏览范围：
                </span>
                <button
                  type="button"
                  aria-pressed={browseRange === "all"}
                  onClick={() => setBrowseRange("all")}
                  className={rangeChipClass(browseRange === "all")}
                >
                  全部
                </button>
                {(
                  [
                    { id: "today", label: "当天" },
                    { id: "3d", label: "过去 3 天" },
                    { id: "7d", label: "过去 7 天" },
                  ] as const
                ).map((range) => (
                  <button
                    key={range.id}
                    type="button"
                    aria-pressed={browseRange === range.id}
                    onClick={() => {
                      setBrowseRange(range.id);
                      setCustomPickerOpen(false);
                    }}
                    className={rangeChipClass(browseRange === range.id)}
                  >
                    {range.label}
                  </button>
                ))}
                <div className="relative inline-flex items-center" ref={customPickerRef}>
                  <button
                    type="button"
                    aria-pressed={customActive}
                    onClick={() => {
                      if (!customActive) switchToCustom();
                      setCustomPickerOpen((v) => !v);
                    }}
                    className={cn(rangeChipClass(customActive), "inline-flex items-center gap-1.5")}
                  >
                    <span>
                      {customActive && customRange
                        ? `自定义 (${customRange.start.slice(5).replace("-", "/")}~${customRange.end.slice(5).replace("-", "/")})`
                        : "自定义"}
                    </span>
                    <Calendar size={12} className="opacity-70" aria-hidden />
                  </button>
                  {customPickerOpen && customRange && (
                    <div className="absolute left-0 top-[calc(100%+8px)] z-50 flex flex-col gap-2.5 rounded-q-card border border-q-border bg-q-surface-strong p-3 shadow-q-xl backdrop-blur-xl animate-scale-in min-w-[280px]">
                      <div className="flex items-center justify-between">
                        <span className="text-xs font-semibold text-q-text-secondary">自定义浏览范围</span>
                        <button
                          type="button"
                          onClick={() => setCustomPickerOpen(false)}
                          className="rounded p-1 text-q-text-muted hover:bg-q-surface-hover hover:text-q-text-primary"
                          aria-label="关闭日期选择"
                        >
                          <X size={13} aria-hidden />
                        </button>
                      </div>
                      <div className="flex items-center gap-2">
                        <input
                          type="date"
                          value={customRange.start}
                          max={todayIso}
                          onChange={(event) => applyCustomRange(event.target.value, customRange.end)}
                          aria-label="浏览范围开始日期"
                          className={dateInputClass}
                        />
                        <span className="text-xs text-q-text-muted shrink-0">至</span>
                        <input
                          type="date"
                          value={customRange.end}
                          max={todayIso}
                          onChange={(event) => applyCustomRange(customRange.start, event.target.value)}
                          aria-label="浏览范围结束日期"
                          className={dateInputClass}
                        />
                      </div>
                      <p className="text-[11px] leading-relaxed text-q-text-muted">
                        浏览筛选只过滤已同步到本地的动态，不改变雷达监控范围，也不会触发 AI 分析。
                      </p>
                    </div>
                  )}
                </div>
              </div>
              <TiboFeedView
                posts={visible}
                onLoadMore={
                  postsExhausted
                    ? undefined
                    : async () => {
                        const last = posts[posts.length - 1];
                        if (!last || postsLoadingMore) return;
                        setPostsLoadingMore(true);
                        try {
                          const more = await listRadarPosts({
                            cursor: `${last.postedAt}:${last.id}`,
                            limit: 80,
                          });
                          const known = new Set(posts.map((post) => post.id));
                          const fresh = more.filter((post) => !known.has(post.id));
                          if (fresh.length === 0) setPostsExhausted(true);
                          else setExtraPosts((current) => [...current, ...fresh]);
                        } finally {
                          setPostsLoadingMore(false);
                        }
                      }
                }
                loadingMore={postsLoadingMore}
                rangeLabel={browseRange === "all" ? "全部" : formatRadarRangeLabel(browseRange)}
                totalCount={browsed.length}
                signalCount={browsed.filter((p) => p.explicitReset || p.filter === "signal").length}
                relatedCount={browsed.filter((p) => p.filter === "related").length}
                unknownCount={browsed.filter((p) => p.filter === "unknown" && !p.explicitReset).length}
                noneCount={browsed.filter((p) => p.filter === "none" && !p.explicitReset).length}
                selected={selected}
                filter={filter}
                onFilter={setFilter}
                onSelect={setSelectedId}
                onTranslate={(postId) => translateMutation.mutate(postId)}
                translatingPostId={translateMutation.isPending ? translateMutation.variables : null}
              />
            </div>
          )}
          {tab === "history" && <HistoryView data={data} />}
        </>
      )}
      <RadarSettingsDrawer
        open={settingsOpen}
        onClose={() => setSettingsOpen(false)}
        data={data}
        returnFocusRef={settingsButtonRef}
      />
    </div>
  );
}

export type RadarTabId = "signal" | "tibo" | "history";
export const RADAR_TABS: Array<{ id: RadarTabId; label: string }> = [
  { id: "signal", label: "当前信号" },
  { id: "tibo", label: "全部动态" },
  { id: "history", label: "历史记录" },
];

/** 自定义日期输入样式；固定宽度避免默认 date 输入过宽把工具栏挤换行。 */
const dateInputClass =
  "h-8 w-[116px] rounded-md border border-q-border bg-q-surface px-2 text-xs text-q-text-primary outline-none focus:border-q-primary";

function formatTime(value: number) {
  return new Date(value).toLocaleString("zh-CN", { hour12: false });
}

function radarModelOptionLabel(item: RadarModelOption): string {
  if (!item.model) return `${item.displayName} · 需添加自定义模型`;
  return `${item.displayName} · ${item.model}`;
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
  const panelRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const panel = panelRef.current;
    if (!panel) return;
    if (open) panel.removeAttribute("inert");
    else panel.setAttribute("inert", "");
  }, [open]);
  return (
    <div
      ref={panelRef}
      className={`radar-collapse ${className}`}
      data-open={open || undefined}
      aria-hidden={!open}
    >
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

/** 帖子信号徽章色调：显式重置 → 红；信号 → 橙；相关/未分类 → 蓝；无关 → 灰 */
function postBadgeTone(post: RadarPost): "danger" | "warning" | "neutral" | "primary" {
  if (post.explicitReset) return "danger";
  if (post.filter === "signal") return "warning";
  if (post.filter === "related") return "primary";
  if (post.filter === "unknown") return "primary";
  if (post.filter === "none") return "neutral";
  return "primary";
}

/* ————————————————— 当前信号 ————————————————— */

function primaryEventRevision(data: RadarSnapshot | undefined, eventId: string | null | undefined): number | null {
  if (!eventId || !data) return null;
  const fromList = data.activeEvents?.find((event) => event.id === eventId)?.stateRevision;
  if (typeof fromList === "number") return fromList;
  if (data.event?.id === eventId) return data.event.stateRevision;
  return null;
}

/** 分来源同步状态：一个来源失败不清除另一来源成功数据。 */
function SourceStatusChips({ data, className }: { data: RadarSnapshot | undefined; className?: string }) {
  const sources = data?.sources ?? [];
  if (sources.length === 0) return null;
  return (
    <div className={cn("flex shrink-0 items-center gap-1.5", className)}>
      {sources.map((source) => (
        <span
          key={source.sourceId}
          title={
            source.lastError
              ? `${source.displayName}：${source.lastError}`
              : source.lastSuccessAt
                ? `${source.displayName} 上次成功 ${formatCompactTime(source.lastSuccessAt)}`
                : `${source.displayName} 尚未同步`
          }
          className={cn(
            "inline-flex items-center gap-1 rounded-q-pill px-2 py-0.5 text-[11px] font-medium",
            source.lastError
              ? "bg-q-danger-soft text-q-danger"
              : source.freshness === "fresh"
                ? "bg-q-success-soft text-q-success"
                : source.freshness === "stale"
                  ? "bg-q-warning-soft text-q-warning"
                  : "bg-q-neutral-soft text-q-text-secondary",
          )}
        >
          {source.displayName}
          {source.lastError
            ? "失败"
            : source.freshness === "fresh"
              ? "正常"
              : source.freshness === "stale"
                ? "缓存"
                : source.freshness === "unknown"
                  ? "待确认"
                  : "未同步"}
        </span>
      ))}
    </div>
  );
}

export function SignalSummaryView({
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
  const [recentOpen, setRecentOpen] = useState(false);
  const [basisOpen, setBasisOpen] = useState(false);
  const [aiOpen, setAiOpen] = useState(false);
  const [quotaOpen, setQuotaOpen] = useState(false);
  const [noticeOpen, setNoticeOpen] = useState(false);
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
  const quotaSummary = radarQuotaSummaryLine(verifications);
  // 关键依据：当前事件关联帖（来源直接信号优先）。
  const relevantPosts = decision?.relevantPostIds
    ? knownPosts.filter((post) => decision.relevantPostIds.includes(post.id))
    : [];
  const confirmCard = useMutation({
    mutationFn: confirmRadarQuotaChange,
    onSuccess: (snapshot) => queryClient.setQueryData(RADAR_SNAPSHOT_QUERY_KEY, snapshot),
  });
  const confirmReset = useMutation({
    mutationFn: () =>
      confirmRadarUserReset({
        eventId: decision?.activeEventId ?? null,
        expectedRevision: primaryEventRevision(data, decision?.activeEventId),
      }),
    onSuccess: (snapshot) => queryClient.setQueryData(RADAR_SNAPSHOT_QUERY_KEY, snapshot),
  });
  const undoReset = useMutation({
    mutationFn: () =>
      undoRadarUserReset({
        eventId: decision?.activeEventId ?? null,
        expectedRevision: primaryEventRevision(data, decision?.activeEventId),
      }),
    onSuccess: (snapshot) => queryClient.setQueryData(RADAR_SNAPSHOT_QUERY_KEY, snapshot),
  });
  const setNoticeHidden = useMutation({
    mutationFn: setRadarNoticeHidden,
    onSuccess: (snapshot) => queryClient.setQueryData(RADAR_SNAPSHOT_QUERY_KEY, snapshot),
  });

  return (
    <div className="radar-console flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto pr-1">
      {/* 当前判断：结论 + 可选时间 + 简短解释；不使用未经校准的置信度仪表 */}
      <section className="glass-panel flex shrink-0 flex-col gap-3 p-4">
        <div className="flex flex-wrap items-center gap-2">
          <h2 className="text-[13px] font-semibold tracking-tight text-q-text-secondary">当前判断</h2>
          {decision ? (
            <span
              className={cn(
                "rounded-q-pill px-2 py-0.5 text-[11px] font-medium",
                radarDecisionToneClass(decision.status),
              )}
            >
              {radarDecisionBadge(decision)}
            </span>
          ) : null}
          {decision?.observationPeriodText ? (
            <span className="rounded-q-pill bg-q-success-soft px-2 py-0.5 text-[10.5px] font-medium text-q-success">
              {decision.observationPeriodText}
            </span>
          ) : null}
          <span className="ml-auto flex shrink-0 items-center gap-2">
            <SourceStatusChips data={data} className="hidden md:flex" />
            {data?.lastSyncedAt ? (
              <span className="tabular-nums text-[11px] text-q-text-muted">
                更新 {formatCompactTime(data.lastSyncedAt)}
              </span>
            ) : null}
          </span>
        </div>
        {decision ? (
          <>
            <p className="text-[18px] font-bold leading-snug text-q-text-primary" data-selectable="true">
              {decision.headline}
            </p>
            {decision.timeText ? (
              <p className="text-[13px] leading-relaxed text-q-text-secondary" data-selectable="true">
                {radarDecisionTimeText(decision)}
              </p>
            ) : null}
            {observedCauseUnknown ? <p className="text-[12px] text-q-text-muted">原因未知</p> : null}
            {decision.verificationHint ? (
              <p className="text-[13px] leading-relaxed text-q-text-secondary">{decision.verificationHint}</p>
            ) : null}
            {data ? (
              <p className="text-[12.5px] leading-relaxed text-q-text-secondary">{radarDeltaImpactLine(data)}</p>
            ) : null}
            {!recentCard && decision.status === "no_signal" && decision.recentSummaryText ? (
              <p className="text-[12.5px] leading-relaxed text-q-text-secondary">{decision.recentSummaryText}</p>
            ) : null}
            {decision.otherActiveEventIds && decision.otherActiveEventIds.length > 0 ? (
              <p className="text-[12.5px] leading-relaxed text-q-text-secondary">
                另有 {decision.otherActiveEventIds.length} 个活动事件，可在历史记录中查看。
              </p>
            ) : null}
            {decision.canConfirmReset || decision.canUndoConfirm ? (
              <div className="flex flex-wrap items-center gap-2 pt-0.5">
                {decision.canConfirmReset ? (
                  <Button variant="secondary" size="sm" onClick={() => setConfirmOpen(true)}>
                    {radarConfirmResetLabel(decision.eventType)}
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

        {/* 两项历史信息：上次重置卡到账 + 最近一次重置 */}
        {decision && (radarBankedChangeRows(decision).length > 0 || recentCard) ? (
          <div className="flex flex-col gap-1.5 rounded-q-control border border-q-border/60 bg-q-surface-strong/40 p-2.5">
            {radarBankedChangeRows(decision).map((change) => (
              <div
                key={`${change.kind}-${change.observedAt}-${change.sourceId}`}
                className="flex items-center gap-2 text-[12.5px]"
              >
                <CreditCard size={14} className="shrink-0 text-q-primary/80" aria-hidden />
                <span className="font-medium text-q-text-secondary">{radarBankedChangeTitle(change)}</span>
                <span className="ml-auto min-w-0 shrink-0 truncate tabular-nums text-q-text-muted">
                  {radarBankedChangeMeta(change, formatCompactTime)}
                </span>
              </div>
            ))}
            {recentCard ? (
              <div className={cn(radarBankedChangeRows(decision).length > 0 && "border-t border-q-border/40 pt-1.5")}>
                <button
                  type="button"
                  className="flex w-full cursor-pointer items-center gap-2 text-left text-[12.5px] group"
                  onClick={() => setRecentOpen((value) => !value)}
                  aria-expanded={recentOpen}
                >
                  <RotateCcw size={14} className="shrink-0 text-q-primary/80" aria-hidden />
                  <span className="font-medium text-q-text-secondary">
                    {recentReset ? "最近一次重置" : "最近一次事件"}
                  </span>
                  <span className="ml-auto flex items-center gap-1.5 truncate tabular-nums text-[12px]">
                    {recentResetAt ? (
                      <>
                        <span className="font-semibold text-q-text-primary">{formatCompactTime(recentResetAt)}</span>
                        <span className="text-q-text-muted"> · {radarConfirmationSourceLabel(recentReset?.confirmationSource)}</span>
                      </>
                    ) : (
                      <span className="text-q-text-muted">{recentMeta}</span>
                    )}
                    <ChevronRight
                      size={13}
                      aria-hidden
                      className={cn(
                        "shrink-0 text-q-text-muted transition-transform duration-200 group-hover:text-q-text-primary",
                        recentOpen && "rotate-90",
                      )}
                    />
                  </span>
                </button>
                <AnimatedCollapse open={recentOpen}>
                  <div className="flex flex-col gap-1.5 pt-2 pl-5.5 text-[12px]">
                    {recentResetAt ? (
                      <p className="leading-relaxed text-q-text-secondary">
                        <span className="font-medium text-q-text-primary">真实时间：</span>
                        <span className="tabular-nums">{formatCompactTime(recentResetAt)}</span>
                      </p>
                    ) : null}
                    <p className="leading-relaxed text-q-text-secondary">
                      <span className="font-medium text-q-text-primary">确认方式：</span>
                      {radarConfirmationSourceLabel(recentCard.confirmationSource)}
                    </p>
                    <p className="leading-relaxed text-q-text-secondary">
                      <span className="font-medium text-q-text-primary">最终状态：</span>
                      {radarCloseReasonLabel(recentCard.closeReason)}
                    </p>
                    {recentCard.analysis?.conclusion ? (
                      <p className="leading-relaxed text-q-text-secondary" data-selectable="true">
                        <span className="font-medium text-q-text-primary">当时结论：</span>
                        {humanizeRadarPostRefs(recentCard.analysis.conclusion, knownPosts)}
                      </p>
                    ) : null}
                  </div>
                </AnimatedCollapse>
              </div>
            ) : null}
          </div>
        ) : null}
      </section>

      {/* 关键依据：来源信号帖直出；AI 解释默认折叠，按需展开 */}
      <section className="radar-slim-panel">
        <button
          type="button"
          className="radar-collapse-trigger"
          onClick={() => setBasisOpen((value) => !value)}
          aria-expanded={basisOpen}
        >
          <ChevronRight size={15} aria-hidden className={cn("radar-collapse-chevron", basisOpen && "is-open")} />
          <h2 className="text-[13.5px] font-semibold tracking-tight text-q-text-primary">依据与原帖</h2>
          <span className="min-w-0 flex-1 text-[12px] text-q-text-muted">
            {relevantPosts.length > 0 ? `当前事件关联 ${relevantPosts.length} 条原帖` : "暂无关联原帖"}
          </span>
          {aiReasoning ? (
            <span className="radar-inline-toggle shrink-0" data-open={aiOpen || undefined} aria-hidden>
              AI {radarAiStatusLabel(ai)}
            </span>
          ) : null}
        </button>
        <AnimatedCollapse open={basisOpen}>
          <div className="radar-slim-content flex flex-col gap-3 pt-2">
            {relevantPosts.length > 0 ? (
              <div className="flex flex-col gap-2">
                {relevantPosts.slice(0, 5).map((post) => (
                  <div key={post.id} className="rounded-q-control border border-q-border bg-q-surface-strong px-3 py-2">
                    <div className="flex items-center gap-2">
                      <StatusBadge tone={postBadgeTone(post)}>{sourceRelationLabel(post)}</StatusBadge>
                      <span className="ml-auto shrink-0 tabular-nums text-[12px] text-q-text-muted">
                        {formatCompactTime(post.postedAt)}
                      </span>
                    </div>
                    <p className="mt-1 text-[13.5px] leading-relaxed text-q-text-primary" data-selectable="true">
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
                {relevantPosts.length > 5 ? (
                  <p className="px-1 text-[11px] text-q-text-muted">…共 {relevantPosts.length} 条，完整列表见「全部动态」。</p>
                ) : null}
              </div>
            ) : (
              <p className="text-[13px] leading-relaxed text-q-text-secondary">当前没有活动事件关联的原帖。</p>
            )}
            {/* AI 解释：作为依据的一部分按需展开，不默认占用大卡片 */}
            <div className="rounded-q-control border border-q-border/70 bg-q-primary-softer/50 px-3 py-2.5">
              <button
                type="button"
                className="radar-inline-toggle"
                data-open={aiOpen || undefined}
                onClick={() => setAiOpen((value) => !value)}
              >
                {aiOpen ? "收起 AI 解释" : "展开 AI 解释"}
                <ChevronDown size={13} aria-hidden />
              </button>
              <AnimatedCollapse open={aiOpen}>
                <div className="flex flex-col gap-2 pt-2">
                  {aiReasoning?.conclusion ? (
                    <>
                      <p className="text-[13px] font-semibold text-q-primary">结论</p>
                      <p className="text-[14px] font-medium leading-relaxed text-q-text-primary" data-selectable="true">
                        {humanizeRadarPostRefs(aiReasoning.conclusion, knownPosts)}
                      </p>
                      {aiReasoning.signalType ? (
                        <p className="text-[12px] text-q-text-muted">信号类型 · {radarSignalTypeLabel(aiReasoning.signalType)}</p>
                      ) : null}
                      {aiReasoning.analysisBasis ? (
                        <>
                          <p className="text-[13px] font-semibold text-q-primary">分析</p>
                          <p className="text-[13.5px] leading-[1.65] text-q-text-secondary" data-selectable="true">
                            {humanizeRadarPostRefs(aiReasoning.analysisBasis, knownPosts)}
                          </p>
                        </>
                      ) : null}
                      <p className="text-[12px] text-q-text-muted">
                        {aiReasoning.model ?? "未知模型"} · {formatTime(aiReasoning.createdAt)}
                        {aiReasoning.rangeKey ? ` · ${formatRadarRangeLabel(aiReasoning.rangeKey)}` : ""}
                      </p>
                      {citedPosts.length > 0 ? (
                        <div className="flex flex-col gap-1.5">
                          <p className="text-[13px] font-semibold text-q-primary">引用原帖</p>
                          {citedPosts.map((post) => (
                            <div key={post.id} className="rounded-q-control border border-q-border bg-q-surface-strong px-3 py-2">
                              <div className="flex items-center gap-2">
                                <span className={aiReasoning?.eventRelation === "none" ? "hb-signal-tag hb-signal-tag-none" : "hb-signal-tag hb-signal-tag-direct"}>
                                  {aiReasoning?.eventRelation === "none" ? "无关信号" : "直接信号"}
                                </span>
                                <span className="ml-auto shrink-0 tabular-nums text-[12px] text-q-text-muted">
                                  {formatCompactTime(post.postedAt)}
                                </span>
                              </div>
                              <p className="mt-1 text-[13px] leading-relaxed text-q-text-primary" data-selectable="true">
                                {post.summary ?? post.translatedText ?? post.text}
                              </p>
                            </div>
                          ))}
                        </div>
                      ) : null}
                      <ListBlock title="支持依据" icon={<ThumbsUp size={13} aria-hidden />} items={aiReasoning.support.map((item) => humanizeRadarPostRefs(item, knownPosts))} />
                      <ListBlock title="反向依据" icon={<ThumbsDown size={13} aria-hidden />} items={aiReasoning.against.map((item) => humanizeRadarPostRefs(item, knownPosts))} />
                      <ListBlock title="不确定性" icon={<HelpCircle size={13} aria-hidden />} items={aiReasoning.uncertainty.map((item) => humanizeRadarPostRefs(item, knownPosts))} />
                    </>
                  ) : (
                    <p className="text-[12.5px] leading-relaxed text-q-text-secondary">
                      {!ai || ai.state === "disabled"
                        ? "AI 未启用：可在「雷达设置」中开启；来源与本机验证不受影响。"
                        : ai.state === "failed"
                          ? (ai.latestError ? `AI 分析失败：${ai.latestError}` : "AI 分析失败，可在下次检查时重试。")
                          : "尚无 AI 分析结果；有新动态并开启 AI 时自动生成。"}
                    </p>
                  )}
                </div>
              </AnimatedCollapse>
            </div>
          </div>
        </AnimatedCollapse>
      </section>

      {/* 本机观察：按需展开 */}
      <section className="radar-slim-panel">
        <button type="button" className="radar-collapse-trigger" onClick={() => setQuotaOpen((value) => !value)} aria-expanded={quotaOpen}>
          <ChevronRight size={15} aria-hidden className={cn("radar-collapse-chevron", quotaOpen && "is-open")} />
          <h2 className="text-[13.5px] font-semibold tracking-tight text-q-text-primary">本机观察</h2>
          <span className="min-w-0 flex-1 truncate text-[12px] text-q-text-muted">{quotaSummary}</span>
        </button>
        <AnimatedCollapse open={quotaOpen}>
          <div className="radar-slim-content flex flex-col pt-1">
            <div className="flex flex-col">
            {verifications.length === 0 ? (
              <p className="text-xs text-q-text-muted">未接入 GPT 额度来源。</p>
            ) : (
              verifications.map((item, index) => (
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
                    <p className="text-[12.5px] leading-relaxed text-q-text-secondary">
                      当前网络无法获取 Codex 额度，不影响来源与 AI 判断。
                    </p>
                  )}
                  {item.note && <p className="text-[12.5px] leading-relaxed text-q-text-secondary">{item.note}</p>}
                  <p className="text-[12.5px] leading-relaxed text-q-text-secondary">
                    {item.bankedResetLabel}
                    {radarAccountBankedChangeNotes(item, formatCompactTime)
                      ? ` · ${radarAccountBankedChangeNotes(item, formatCompactTime)}`
                      : ""}
                  </p>
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
              ))
            )}
            </div>
          </div>
        </AnimatedCollapse>
      </section>

      {/* CodexRadar 公告：低强调折叠；隐藏后不再渲染占位条 */}
      {!data?.noticeHidden && (
        <section className="radar-slim-panel radar-notice-card">
          <button type="button" className="radar-collapse-trigger" onClick={() => setNoticeOpen((value) => !value)} aria-expanded={noticeOpen}>
            <ChevronRight size={15} aria-hidden className={cn("radar-collapse-chevron", noticeOpen && "is-open")} />
            <h2 className="whitespace-nowrap text-[13px] font-medium tracking-tight text-q-text-secondary">
              {notice ? (notice.isCurrent ? "CodexRadar 公告" : "CodexRadar 最近公告") : "CodexRadar 当前无公告"}
            </h2>
            <span className="ml-2 min-w-0 flex-1 truncate text-[12.5px] text-q-text-secondary">
              {notice ? notice.headline : "帖子同步正常"}
            </span>
          </button>
          <AnimatedCollapse open={noticeOpen}>
            <div className="radar-slim-content flex flex-col gap-2 pt-2">
              {notice ? (
                <>
                  <p className="text-[13px] leading-relaxed text-q-text-secondary" data-selectable="true">
                    <span className="font-semibold text-q-text-primary">{notice.headline}</span>
                    {notice.lead ? <span> · {notice.lead}</span> : null}
                  </p>
                  <div className="flex flex-wrap items-center gap-3 text-[12px] text-q-text-muted">
                    <span className="tabular-nums">
                      {notice.isCurrent
                        ? notice.updatedAt
                          ? `更新 ${formatTime(notice.updatedAt)}`
                          : "当前公告"
                        : notice.updatedAt
                          ? `上次出现于 ${formatTime(notice.updatedAt)}`
                          : "历史公告"}
                    </span>
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
                    <button
                      type="button"
                      className="cursor-pointer font-medium text-q-text-muted hover:text-q-primary hover:underline disabled:opacity-50"
                      disabled={setNoticeHidden.isPending}
                      aria-label="隐藏 CodexRadar 公告"
                      onClick={() => setNoticeHidden.mutate(true)}
                    >
                      隐藏
                    </button>
                  </div>
                </>
              ) : (
                <button
                  type="button"
                  className="w-fit cursor-pointer text-[12px] font-medium text-q-text-muted hover:text-q-primary hover:underline disabled:opacity-50"
                  disabled={setNoticeHidden.isPending}
                  aria-label="隐藏 CodexRadar 公告"
                  onClick={() => setNoticeHidden.mutate(true)}
                >
                  隐藏
                </button>
              )}
            </div>
          </AnimatedCollapse>
        </section>
      )}

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
            <p className="text-sm font-semibold text-q-text-primary">
              {radarConfirmResetDialogTitle(decision?.eventType)}
            </p>
            <p className="mt-2 text-xs leading-relaxed text-q-text-secondary">
              {radarConfirmResetDialogBody(decision?.eventType)}
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

/** 决策状态徽章色系映射：与状态语义一致的色调。 */
function radarDecisionToneClass(status: string): string {
  switch (status) {
    case "landed_observed":
    case "user_confirmed":
      return "bg-q-success-soft text-q-success";
    case "landed_claimed":
    case "expected_time_passed":
    case "unscheduled_reset":
    case "possible_reset":
      return "bg-q-warning-soft text-q-warning";
    case "watching":
    case "upcoming":
    case "claimed_unverified":
      return "bg-q-primary-soft text-q-primary";
    case "no_signal":
    case "unchecked":
    case "source_unavailable":
    case "pending_analysis":
    case "expired":
    default:
      return "bg-q-neutral-soft text-q-text-secondary";
  }
}

/* ————————————————— 历史记录 ————————————————— */

/** 事件结果标签：区分已观察落地 / 用户确认 / 已撤回 / 超时未验证等。 */
function eventOutcome(
  event: RadarHistoryEvent["event"],
): { tone: "success" | "warning" | "neutral" | "primary"; label: string } {
  if (event.closedAt == null) {
    if (event.observedResetAt != null) return { tone: "success", label: "已观察落地 · 观察期" };
    if (event.userConfirmedResetAt != null) return { tone: "success", label: "用户确认 · 观察期" };
    return { tone: "primary", label: "进行中" };
  }
  if (event.observedResetAt != null) return { tone: "success", label: "已观察落地" };
  if (event.userConfirmedResetAt != null) return { tone: "success", label: "用户确认" };
  switch (event.closeReason) {
    case "信号取消或失效":
      return { tone: "warning", label: "已撤回" };
    case "timeout_no_signal":
    case "timeout_unverified":
    case "timeout":
      return { tone: "neutral", label: "超时未验证" };
    case "claimed_unverified":
      return { tone: "warning", label: "声称未验证" };
    case "invalid_historical_replay":
      return { tone: "neutral", label: "历史重放" };
    case "出现新一轮重置信号":
      return { tone: "primary", label: "被新信号替代" };
    default:
      return { tone: "neutral", label: radarCloseReasonLabel(event.closeReason) };
  }
}

export function HistoryView({ data }: { data: RadarSnapshot | undefined }) {
  const [page, setPage] = useState<RadarHistory | null>(null);
  const events = page?.events ?? data?.history.events ?? [];
  const hasMore = page?.hasMore ?? data?.history.hasMore ?? false;
  const nextCursor = page?.nextCursor ?? data?.history.nextCursor ?? null;
  const checks = data?.checks ?? [];
  const knownPosts = data?.posts ?? [];
  const [expanded, setExpanded] = useState<string | null>(null);
  const [logsOpen, setLogsOpen] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  useEffect(() => {
    setPage(null);
  }, [data?.history.events]);
  const loadMore = async () => {
    if (!nextCursor || loadingMore) return;
    setLoadingMore(true);
    try {
      const more = await listRadarHistory({ cursor: nextCursor, limit: 20 });
      setPage({
        events: [...events, ...more.events],
        nextCursor: more.nextCursor ?? null,
        hasMore: more.hasMore ?? false,
      });
    } finally {
      setLoadingMore(false);
    }
  };
  const [analysisLoading, setAnalysisLoading] = useState<string | null>(null);
  const [analysisError, setAnalysisError] = useState<string | null>(null);
  const loadAnalyses = async (eventId: string, cursor: string) => {
    setAnalysisLoading(eventId); setAnalysisError(null);
    try {
      const more = await listRadarEventAnalyses(eventId,cursor);
      setPage({events:events.map(item => item.event.id === eventId ? {...item,analyses:[...item.analyses,...more.items],analysesNextCursor:more.nextCursor} : item),hasMore,nextCursor});
    } catch (error) { setAnalysisError(String(error)); }
    finally { setAnalysisLoading(null); }
  };
  if (events.length === 0) {
    return (
      <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto pr-1 pb-6">
        <div className="glass-panel flex shrink-0 flex-col items-center justify-center gap-2 p-8">
          <History size={28} aria-hidden className="text-q-text-muted" />
          <p className="text-sm text-q-text-secondary">暂无历史事件</p>
          <p className="text-xs text-q-text-muted">发现重置信号并完成研判后，事件会按时间归档在这里。</p>
        </div>
        <HistoryCheckLogs checks={checks} open={logsOpen} onToggle={() => setLogsOpen((value) => !value)} />
      </div>
    );
  }
  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto pr-1 pb-6">
      <div className="radar-timeline-container shrink-0">
        {events.map((item) => {
          const open = expanded === item.event.id;
          const outcome = eventOutcome(item.event);
          return (
            <div key={item.event.id} className="radar-timeline-item">
              <button
                type="button"
                className="radar-timeline-item-trigger"
                onClick={() => setExpanded(open ? null : item.event.id)}
                aria-expanded={open}
              >
                <ChevronRight size={15} aria-hidden className={cn("radar-collapse-chevron", open && "is-open")} />
                <StatusBadge tone={item.event.eventType === "banked_reset" ? "primary" : "warning"}>
                  {item.event.eventType === "banked_reset" ? "重置卡" : "额度重置"}
                </StatusBadge>
                <span className="min-w-0 flex-1 truncate text-left text-[13.5px] font-semibold text-q-text-primary">
                  {item.event.title}
                </span>
                <StatusBadge tone={outcome.tone}>{outcome.label}</StatusBadge>
                <span className="shrink-0 tabular-nums text-[12px] text-q-text-muted">
                  {formatCompactTime(item.event.firstSignalAt)}
                </span>
              </button>
              <AnimatedCollapse open={open}>
                <div className="radar-timeline-item-content flex flex-col gap-3.5 pt-2">
                  {/* 时间线：时间变化与关键节点 */}
                  <div className="flex flex-col gap-1.5">
                    <p className="text-[12.5px] font-semibold text-q-text-primary">事件时间线</p>
                    {item.event.timeline.map((node, index) => (
                      <div key={`${node.at}-${index}`} className="flex items-baseline gap-2 text-[12.5px]">
                        <span className="w-[92px] shrink-0 tabular-nums text-q-text-muted">{formatCompactTime(node.at)}</span>
                        <span
                          className={cn(
                            "h-1.5 w-1.5 shrink-0 rounded-full",
                            node.kind === "observed" || node.kind === "confirmed"
                              ? "bg-q-success"
                              : node.kind === "closed"
                                ? "bg-q-neutral"
                                : "bg-q-primary",
                          )}
                        />
                        <span className="text-q-text-secondary">{node.label}</span>
                      </div>
                    ))}
                    {item.event.expectedAt ? (
                      <p className="text-[12px] tabular-nums text-q-text-muted">
                        预告重置时间 {formatCompactTime(item.event.expectedAt)}（北京时间）
                      </p>
                    ) : null}
                  </div>
                  {/* 原始信号帖 */}
                  {item.evidencePosts.length > 0 ? (
                    <div className="flex flex-col gap-1.5">
                      <p className="text-[12.5px] font-semibold text-q-text-primary">原始信号（{item.evidencePosts.length} 条）</p>
                      <div className="flex flex-col gap-1.5">
                        {item.evidencePosts.slice(0, 6).map((post) => (
                          <div key={post.id} className="rounded-q-control border border-q-border bg-q-surface-strong px-3 py-2">
                            <div className="flex items-center gap-2">
                              <StatusBadge tone={postBadgeTone(post)}>{sourceRelationLabel(post)}</StatusBadge>
                              <span className="ml-auto shrink-0 tabular-nums text-[12px] text-q-text-muted">
                                {formatCompactTime(post.postedAt)}
                              </span>
                            </div>
                            <p className="mt-1 line-clamp-3 text-[13px] leading-relaxed text-q-text-primary" data-selectable="true">
                              {post.summary ?? post.translatedText ?? post.text}
                            </p>
                            {post.url ? (
                              <button
                                type="button"
                                className="mt-1 inline-flex cursor-pointer items-center gap-1 text-[12px] text-q-primary hover:underline"
                                onClick={() => void openExternalUrl(post.url).catch(() => {})}
                              >
                                <ExternalLink size={12} aria-hidden />
                                查看原帖
                              </button>
                            ) : null}
                          </div>
                        ))}
                        {item.evidencePosts.length > 6 ? (
                          <p className="px-1 text-[11px] text-q-text-muted">…共 {item.evidencePosts.length} 条</p>
                        ) : null}
                      </div>
                    </div>
                  ) : null}
                  {/* 本机观察 */}
                  {item.quotaObservations.length > 0 || item.bankedObservations.length > 0 ? (
                    <div className="flex flex-col gap-1.5">
                      <p className="text-[12.5px] font-semibold text-q-text-primary">本机观察</p>
                      {item.quotaObservations.map((observation) => (
                        <p key={observation.id} className="text-[12.5px] leading-relaxed text-q-text-secondary" data-selectable="true">
                          <span className="tabular-nums">{formatCompactTime(observation.observedAt)}</span>
                          {" · "}
                          {observation.accountName}
                          {" · "}
                          {quotaBadgeLabel(
                            observation.classification,
                            observation.userConfirmedAt != null ? "user_confirmed" : "unknown",
                            observation.observedAt,
                          )}
                          {quotaCorrelationLabel(observation.temporalCorrelation)
                            ? ` · ${quotaCorrelationLabel(observation.temporalCorrelation)}`
                            : ""}
                        </p>
                      ))}
                      {item.bankedObservations.map((observation, index) => (
                        <p key={`banked-${index}`} className="text-[12.5px] leading-relaxed text-q-text-secondary" data-selectable="true">
                          <span className="tabular-nums">{formatCompactTime(observation.observedAt)}</span>
                          {" · "}
                          {observation.accountName}
                          {" · "}
                          {observation.kind === "grant"
                            ? `重置卡 ${observation.previousCount} → ${observation.currentCount}（到账）`
                            : `重置卡 ${observation.previousCount} → ${observation.currentCount}（消耗）`}
                        </p>
                      ))}
                    </div>
                  ) : null}
                  {analysisError ? <p role="alert" className="text-xs text-q-danger">{analysisError}</p> : null}
                  {/* 当时的 AI 研判记录 */}
                  {item.analyses.length > 0 ? (
                    <div className="flex flex-col gap-2 pt-1">
                      <div className="flex items-center gap-1.5 text-[12.5px] font-semibold text-q-text-primary">
                        <Sparkles size={13} className="shrink-0 text-q-primary" aria-hidden />
                        <span>当时的 AI 研判记录</span>
                      </div>
                      <div className="flex flex-col gap-2">
                        {item.analyses.map((analysis) => (
                          <div key={analysis.id} className="rounded-q-control border border-q-border/70 bg-q-primary-softer/35 px-3 py-2.5">
                            <p className="text-[12.5px] font-medium leading-relaxed text-q-text-primary" data-selectable="true">
                              {humanizeRadarPostRefs(analysis.conclusion, knownPosts)}
                            </p>
                            {analysis.analysisBasis ? (
                              <p className="mt-1.5 text-[12px] leading-relaxed text-q-text-secondary" data-selectable="true">
                                {humanizeRadarPostRefs(analysis.analysisBasis, knownPosts)}
                              </p>
                            ) : null}
                            <p className="mt-1.5 text-[11px] tabular-nums text-q-text-muted">
                              {analysis.model ?? "未知模型"} · {formatTime(analysis.createdAt)}
                              {analysis.signalType && analysis.signalType !== "unknown"
                                ? ` · ${radarSignalTypeLabel(analysis.signalType)}`
                                : ""}
                            </p>
                      </div>
                        ))}
                            {item.analysesNextCursor ? <Button variant="secondary" size="sm" disabled={analysisLoading === item.event.id} onClick={() => void loadAnalyses(item.event.id,item.analysesNextCursor!)}>{analysisLoading === item.event.id ? "加载中…" : "加载更早的研判记录"}</Button> : null}
                      </div>
                    </div>
                  ) : null}
                </div>
              </AnimatedCollapse>
            </div>
          );
        })}
      </div>
      {hasMore ? (
        <Button variant="secondary" size="sm" className="self-center" disabled={loadingMore} onClick={() => void loadMore()}>
          {loadingMore ? "加载中…" : "加载更多历史"}
        </Button>
      ) : null}
      <HistoryCheckLogs checks={checks} open={logsOpen} onToggle={() => setLogsOpen((value) => !value)} />
    </div>
  );
}

function HistoryCheckLogs({
  checks,
  open,
  onToggle,
}: {
  checks: NonNullable<RadarSnapshot["checks"]>;
  open: boolean;
  onToggle: () => void;
}) {
  return (
    <section className="radar-slim-panel shrink-0">
      <button type="button" className="radar-collapse-trigger" onClick={onToggle} aria-expanded={open}>
        <ChevronRight size={15} aria-hidden className={cn("radar-collapse-chevron", open && "is-open")} />
        <h2 className="text-[13px] font-medium text-q-text-secondary">技术检查日志</h2>
        <span className="min-w-0 flex-1 text-[12px] text-q-text-muted">共 {checks.length} 次</span>
      </button>
      <AnimatedCollapse open={open}>
        <div className="radar-slim-content flex flex-col pt-1">
          <div className="radar-history-scroll flex flex-col">
            {checks.length === 0 && <p className="text-xs text-q-text-muted">还没有检查记录</p>}
            {checks.map((check) => (
              <div
                key={check.id}
                className="flex flex-wrap items-center gap-x-4 gap-y-1 border-b border-q-border/60 py-2 text-[12.5px] last:border-b-0"
              >
                <span className="w-[104px] shrink-0 tabular-nums text-q-text-secondary">
                  {formatCompactTime(check.startedAt)}
                </span>
                <StatusBadge
                  tone={check.status === "success" ? "success" : check.status === "running" ? "primary" : "danger"}
                >
                  {check.status === "success" ? "成功" : check.status === "running" ? "进行中" : "失败"}
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
  );
}

/* ————————————————— 雷达设置抽屉 ————————————————— */

function RadarSettingsDrawer({
  open,
  onClose,
  data,
  returnFocusRef,
}: {
  open: boolean;
  onClose: () => void;
  data: RadarSnapshot | undefined;
  returnFocusRef: { current: HTMLButtonElement | null };
}) {
  const queryClient = useQueryClient();
  // 草稿模式：打开时从已保存偏好初始化；只有点「保存」才写入，刷新/关闭不偷偷保存。
  const [analyze, setAnalyze] = useState(false);
  const [sourceId, setSourceId] = useState("");
  const [modelChoice, setModelChoice] = useState("");
  const [userPrompt, setUserPrompt] = useState("");
  const [backgroundCheck, setBackgroundCheck] = useState(true);
  const initialized = useRef(false);
  useEffect(() => {
    if (!open || !data || initialized.current) return;
    initialized.current = true;
    setAnalyze(data.analysisPrefs.analyze);
    const readyModels = (data.models ?? []).filter((item) => item.ready);
    const saved = readyModels.find(
      (item) => item.sourceId === data.analysisPrefs.sourceId && item.model === (data.analysisPrefs.model ?? ""),
    );
    const fallback = readyModels.find((item) => item.model) ?? readyModels[0];
    const pick = saved ?? fallback;
    setSourceId(pick ? pick.sourceId : "");
    setModelChoice(pick ? pick.model : "");
    setUserPrompt(data.analysisPrefs.userPrompt ?? data.analysisPrefs.defaultUserPrompt ?? "");
    setBackgroundCheck(data.analysisPrefs.backgroundCheck);
  }, [open, data]);
  useEffect(() => {
    if (!open) initialized.current = false;
  }, [open]);
  const panelRef = useRef<HTMLDivElement>(null);
  const closeButtonRef = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (!open) return;
    const previous = document.activeElement as HTMLElement | null;
    closeButtonRef.current?.focus();
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onClose();
      }
      if (event.key !== "Tab" || !panelRef.current) return;
      const focusable = Array.from(
        panelRef.current.querySelectorAll<HTMLElement>(
          'button:not([disabled]), [href], input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])',
        ),
      ).filter((item) => item.offsetParent !== null);
      if (focusable.length === 0) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
      (returnFocusRef.current ?? previous)?.focus();
    };
  }, [open, onClose, returnFocusRef]);

  const modelOptions = (data?.models ?? []).filter((item) => item.ready);
  const selectedModelOption =
    modelOptions.find((item) => item.sourceId === sourceId && item.model === modelChoice) ??
    modelOptions.find((item) => item.sourceId === sourceId && item.model) ??
    modelOptions.find((item) => item.model) ??
    modelOptions[0] ??
    null;
  const prefs = data?.analysisPrefs;
  const dirty =
    analyze !== (prefs?.analyze ?? false) ||
    backgroundCheck !== (prefs?.backgroundCheck ?? true) ||
    (selectedModelOption?.sourceId ?? "") !== (prefs?.sourceId ?? "") ||
    (selectedModelOption?.model ?? "") !== (prefs?.model ?? "") ||
    userPrompt !== (prefs?.userPrompt ?? "");
  const saveMutation = useMutation({
    mutationFn: () =>
      saveRadarAnalysisPrefs({
        analyze,
        sourceId: selectedModelOption?.sourceId ?? null,
        model: selectedModelOption?.model ?? null,
        userPrompt,
        backgroundCheck,
      }),
    onSuccess: (snapshot) => {
      queryClient.setQueryData(RADAR_SNAPSHOT_QUERY_KEY, snapshot);
      onClose();
    },
  });

  const byId = useMemo(() => new Map((data?.posts ?? []).map((post) => [post.id, post])), [data?.posts]);
  const pickPosts = (ids: string[]) =>
    ids.map((id) => byId.get(id)).filter((post): post is RadarPost => Boolean(post));
  const analysisInput = pickPosts(data?.analysisGroups.newPostIds ?? []);
  const contextInput = pickPosts(data?.analysisGroups.eventContextIds ?? []);
  const historicalInput = pickPosts(data?.analysisGroups.historicalContextIds ?? []);

  if (!open) return null;
  return (
    <div
      className="fixed inset-0 z-50 flex justify-end bg-black/35 backdrop-blur-[2px]"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && !saveMutation.isPending) onClose();
      }}
    >
      <div
        ref={panelRef}
        role="dialog"
        aria-modal="true"
        aria-label="雷达设置"
        className="flex h-full w-full max-w-[460px] flex-col border-l border-q-border bg-q-surface-solid shadow-q-xl"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <div className="flex shrink-0 items-center gap-2 border-b border-q-border/70 px-4 py-3">
          <Settings2 size={16} aria-hidden className="text-q-primary" />
          <h2 className="text-[15px] font-semibold tracking-tight text-q-text-primary">雷达设置</h2>
          <button
            ref={closeButtonRef}
            type="button"
            aria-label="关闭雷达设置"
            className="ml-auto cursor-pointer rounded p-1.5 text-q-text-muted hover:bg-q-surface-hover hover:text-q-text-primary"
            onClick={onClose}
          >
            <X size={15} aria-hidden />
          </button>
        </div>
        <div className="flex min-h-0 flex-1 flex-col gap-5 overflow-y-auto px-4 py-4">
          {/* 后台自动检查 */}
          <section className="flex flex-col gap-2.5">
            <div className="flex items-center justify-between gap-4">
              <div className="min-w-0">
                <p className="text-sm font-medium text-q-text-primary">后台自动检查</p>
                <p className="mt-1 text-[12.5px] leading-relaxed text-q-text-secondary">
                  应用运行期间每 15 分钟自动同步来源并按需分析；不依赖打开的页面。关闭后「立即检查」仍可手动使用。
                </p>
              </div>
              <Switch
                checked={backgroundCheck}
                onCheckedChange={setBackgroundCheck}
                label="后台自动检查（15 分钟）"
              />
            </div>
            {data?.analysisPrefs.backgroundNextAt ? (
              <p className="text-[11.5px] tabular-nums text-q-text-muted">
                下次后台检查：{formatTime(data.analysisPrefs.backgroundNextAt)}
              </p>
            ) : null}
          </section>

          {/* AI 分析 */}
          <section className="flex flex-col gap-3.5 border-t border-q-border/60 pt-4">
            <h3 className="text-[13px] font-semibold tracking-tight text-q-text-secondary">AI 辅助分析（可选）</h3>
            <div className="flex items-center justify-between gap-4">
              <p className="text-sm font-medium text-q-text-primary">开启 AI 分析</p>
              <Switch checked={analyze} onCheckedChange={setAnalyze} label="开启 AI 分析" />
            </div>
            <AiModelSettings
              models={modelOptions}
              selected={selectedModelOption}
              endpoints={data?.chatEndpoints ?? []}
              onSelected={(nextSource, nextModel) => {
                setSourceId(nextSource);
                setModelChoice(nextModel);
              }}
            />
            <p className="text-[12.5px] leading-relaxed text-q-text-secondary">
              开启后只分析监控窗口（最近 72 小时）内尚未消费的新帖；关闭时仍同步来源，不调用模型，新帖保持待分析。
            </p>
          </section>

          {/* 语义提示 */}
          <section className="flex flex-col gap-2 border-t border-q-border/60 pt-4">
            <div className="flex flex-wrap items-center gap-2">
              <h3 className="text-[13px] font-semibold tracking-tight text-q-text-secondary">语义提示</h3>
              <span className="rounded-q-pill bg-q-neutral-soft px-2 py-0.5 text-[11px] text-q-text-secondary">
                {userPrompt === (prefs?.defaultUserPrompt ?? "") ? "默认" : userPrompt.trim() ? "自定义" : "未设置"}
              </span>
            </div>
            <p className="text-[12.5px] leading-relaxed text-q-text-secondary">
              会随每次分析发给模型，补充你认为算强重置信号的措辞；不改变只发送英文原文、时间和链接的限制。
            </p>
            <textarea
              aria-label="语义提示"
              value={userPrompt}
              onChange={(event) => setUserPrompt(event.target.value.slice(0, 4000))}
              rows={6}
              maxLength={4000}
              placeholder="例如：提到 dashboard、milestone、Hold on to your Codex 时视为即将重置的强信号。"
              className="min-h-[120px] w-full resize-y rounded-q-control border border-q-border bg-q-surface-strong px-3 py-2.5 text-[13px] leading-relaxed text-q-text-primary outline-none focus:border-q-primary"
            />
            <div className="flex items-center justify-between">
              <p className="text-[11px] text-q-text-muted">{userPrompt.length}/4000</p>
              <button
                type="button"
                className="cursor-pointer text-xs font-medium text-q-text-secondary hover:text-q-primary hover:underline"
                onClick={() => setUserPrompt(prefs?.defaultUserPrompt ?? "")}
              >
                恢复默认
              </button>
            </div>
          </section>

          {/* 下次分析输入审计 */}
          <section className="flex flex-col gap-2.5 border-t border-q-border/60 pt-4">
            <h3 className="text-[13px] font-semibold tracking-tight text-q-text-secondary">下次分析输入（监控窗口）</h3>
            <div className="flex items-start gap-2.5 rounded-q-control border border-q-border bg-q-primary-softer px-3 py-2.5">
              <ShieldCheck size={15} aria-hidden className="mt-0.5 shrink-0 text-q-primary" />
              <p className="text-[12.5px] leading-relaxed text-q-text-secondary">
                只发送英文原文、时间和原帖链接。不发送 CodexRadar 的中文翻译、信号标签或模型语境解读。
              </p>
            </div>
            {analysisInput.length === 0 && contextInput.length === 0 && historicalInput.length === 0 ? (
              <p className="text-[12.5px] leading-relaxed text-q-text-secondary">监控窗口内没有待分析的新材料。</p>
            ) : (
              <div className="flex flex-col gap-3">
                <PostGroupPreview title="待分析新帖" subtitle="真正未消费，才能创建或推进事件" posts={analysisInput} />
                {contextInput.length > 0 && (
                  <PostGroupPreview title="事件上下文" subtitle="已关联当前事件，不能单独作为新证据" posts={contextInput} />
                )}
                {historicalInput.length > 0 && (
                  <PostGroupPreview title="历史上下文" subtitle="已关闭事件帖，只能生成历史解释" posts={historicalInput} />
                )}
              </div>
            )}
          </section>
        </div>
        <div className="flex shrink-0 items-center justify-end gap-2 border-t border-q-border/70 px-4 py-3">
          {saveMutation.error ? (
            <p
              className="mr-auto max-w-[55%] truncate text-xs text-q-danger"
              title={ipcErrorMessage(saveMutation.error, "保存失败")}
            >
              {ipcErrorMessage(saveMutation.error, "保存失败")}
            </p>
          ) : dirty ? (
            <p className="mr-auto text-xs text-q-warning">有未保存的修改</p>
          ) : null}
          <Button variant="ghost" size="sm" disabled={saveMutation.isPending} onClick={onClose}>
            取消
          </Button>
          <Button size="sm" disabled={saveMutation.isPending} onClick={() => saveMutation.mutate()}>
            {saveMutation.isPending ? "保存中…" : "保存设置"}
          </Button>
        </div>
      </div>
    </div>
  );
}

/** AI 模型选择 + 测试连接 + 自定义模型 + 独立对话接入（设置抽屉内复用）。 */
function AiModelSettings({
  models,
  selected,
  endpoints,
  onSelected,
}: {
  models: RadarModelOption[];
  selected: RadarModelOption | null;
  endpoints: RadarChatEndpoint[];
  onSelected: (sourceId: string, model: string) => void;
}) {
  const [testFeedback, setTestFeedback] = useState<{ kind: "ok" | "error"; text: string } | null>(null);
  useEffect(() => {
    setTestFeedback(null);
  }, [selected?.sourceId, selected?.model]);
  const testSelected = useMutation({
    mutationFn: () => {
      if (!selected?.sourceId || !selected.model) {
        return Promise.reject(new Error("请先选择一个具体模型，或在下方添加自定义模型"));
      }
      return testRadarModel({ sourceId: selected.sourceId, model: selected.model });
    },
    onSuccess: () => setTestFeedback({ kind: "ok", text: "连接成功，当前模型可用。" }),
    onError: (error) => setTestFeedback({ kind: "error", text: ipcErrorMessage(error, "测试失败") }),
  });
  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-col gap-1.5 text-sm">
        <span className="font-medium text-q-text-primary">分析模型</span>
        <div className="flex flex-wrap items-center gap-2">
          <select
            aria-label="分析模型"
            value={selected ? `${selected.sourceId}|${selected.model}` : ""}
            onChange={(event) => {
              const [nextSource, ...rest] = event.target.value.split("|");
              onSelected(nextSource, rest.join("|"));
            }}
            className="h-10 min-w-0 flex-1 cursor-pointer rounded-q-control border border-q-border bg-q-surface-strong px-3 text-sm text-q-text-primary outline-none focus:border-q-primary"
          >
            {models.length === 0 && (
              <option value="">请先在平台中心接入对话 API Key，或添加其他对话接入</option>
            )}
            {models.some((item) => item.kind !== "endpoint") ? (
              <optgroup label="已接入平台">
                {models
                  .filter((item) => item.kind !== "endpoint")
                  .map((item) => (
                    <option key={`${item.sourceId}|${item.model}`} value={`${item.sourceId}|${item.model}`}>
                      {radarModelOptionLabel(item)}
                    </option>
                  ))}
              </optgroup>
            ) : null}
            {models.some((item) => item.kind === "endpoint") ? (
              <optgroup label="其他对话接入">
                {models
                  .filter((item) => item.kind === "endpoint")
                  .map((item) => (
                    <option key={`${item.sourceId}|${item.model}`} value={`${item.sourceId}|${item.model}`}>
                      {radarModelOptionLabel(item)}
                    </option>
                  ))}
              </optgroup>
            ) : null}
          </select>
          <Button
            type="button"
            variant="secondary"
            size="sm"
            className="shrink-0"
            disabled={!selected?.sourceId || !selected.model || testSelected.isPending}
            onClick={() => testSelected.mutate()}
          >
            {testSelected.isPending ? "测试中…" : "测试连接"}
          </Button>
        </div>
        {testFeedback ? (
          <p className={cn("text-xs leading-relaxed", testFeedback.kind === "ok" ? "text-q-success" : "text-q-danger")}>
            {testFeedback.text}
          </p>
        ) : (
          <p className="text-[12px] leading-relaxed text-q-text-secondary">
            只列出当前凭据可用的对话模型。测试连接会发送一次极小请求，可能消耗少量额度。
          </p>
        )}
      </div>
      <CustomModelPanel
        models={models}
        selected={selected}
        onAdded={(model) => selected && onSelected(selected.sourceId, model)}
      />
      <ChatEndpointPanel endpoints={endpoints} onAdded={(nextSource, nextModel) => onSelected(nextSource, nextModel)} />
    </div>
  );
}

/* ————————————————— Tibo 动态（沿用原界面） ————————————————— */

export function TiboFeedView({
  posts,
  rangeLabel,
  totalCount,
  signalCount,
  relatedCount,
  unknownCount,
  noneCount,
  selected,
  filter,
  onFilter,
  onSelect,
  onTranslate,
  translatingPostId,
  onLoadMore,
  loadingMore,
}: {
  posts: RadarPost[];
  rangeLabel: string;
  totalCount: number;
  signalCount: number;
  relatedCount: number;
  unknownCount: number;
  noneCount: number;
  selected: RadarPost | null;
  filter: TiboFilter;
  onFilter: (value: TiboFilter) => void;
  onSelect: (id: string) => void;
  onTranslate?: (id: string) => void;
  translatingPostId?: string | null;
  onLoadMore?: () => void | Promise<void>;
  loadingMore?: boolean;
}) {
  const chips: Array<{ id: TiboFilter; label: string; count: number }> = [
    { id: "all", label: "全部", count: totalCount },
    { id: "signal", label: "直接信号", count: signalCount },
    { id: "related", label: "间接信号", count: relatedCount },
    ...(unknownCount > 0 ? [{ id: "unknown" as const, label: "未分类", count: unknownCount }] : []),
    { id: "none", label: "无关信号", count: noneCount },
  ];
  useEffect(() => {
    if (filter === "unknown" && unknownCount === 0) {
      onFilter("all");
    }
  }, [filter, unknownCount, onFilter]);
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
      {/* 筛选 chips：带计数（不随列表滚走）；缺少标签的帖子归入「未分类」，不当成无关 */}
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
              <p className="px-1 py-6 text-center text-xs text-q-text-muted">
                {totalCount > 0 ? "该筛选分类下暂无动态" : "点「立即检查」同步 CodexRadar。"}
              </p>
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
            {onLoadMore ? (
              <button
                type="button"
                className="w-full rounded-[14px] border border-q-border bg-q-surface-strong px-3 py-2 text-[12.5px] text-q-text-secondary hover:border-q-border-selected"
                disabled={loadingMore}
                onClick={() => void onLoadMore()}
              >
                {loadingMore ? "加载中…" : "加载更多动态"}
              </button>
            ) : null}
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
                <div className="flex items-center justify-between">
                  <p className="text-xs font-medium text-q-text-secondary">中文翻译</p>
                  {!selected.translatedText && onTranslate && (
                    <button
                      type="button"
                      disabled={translatingPostId === selected.id}
                      onClick={() => onTranslate(selected.id)}
                      className="inline-flex cursor-pointer items-center gap-1 rounded px-2 py-0.5 text-xs text-q-primary transition-colors hover:bg-q-primary-softer disabled:opacity-50"
                    >
                      <Sparkles size={12} aria-hidden />
                      {translatingPostId === selected.id ? "翻译中..." : "AI 翻译"}
                    </button>
                  )}
                </div>
                <div
                  className="rounded-q-control border border-q-border bg-q-surface-strong px-3.5 py-3 text-[13px] leading-relaxed text-q-text-primary"
                  data-selectable="true"
                >
                  {selected.translatedText ?? selected.summary ?? "尚无中文翻译（可点击上方「AI 翻译」生成）"}
                </div>
              </div>

              {selected.context ? (
                <div className="flex flex-col gap-2">
                  <p className="text-xs font-medium text-q-text-secondary">回复 / 引用上下文</p>
                  <div
                    className="whitespace-pre-wrap rounded-q-control border border-q-border bg-q-surface-muted px-3.5 py-2.5 text-[12.5px] leading-relaxed text-q-text-secondary"
                    data-selectable="true"
                  >
                    {selected.context}
                  </div>
                </div>
              ) : null}

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
                <Button variant="secondary" size="sm" onClick={() => void openExternalUrl("https://www.willcodexquotareset.com/")}>
                  <ExternalLink size={14} aria-hidden />
                  WillCodex 来源
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

/* ————————————————— 自定义分析模型 / 独立对话接入 ————————————————— */

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
              {selected.kind === "endpoint"
                ? "给当前对话接入再添加一个模型名：先验证连接，通过后保存即可加入上方下拉。"
                : "添加当前平台支持的任意模型名：先验证连接（发送一次极小请求），通过后保存即可加入上方下拉。"}
            </p>
            <div className="flex flex-wrap items-center gap-2">
              <input
                value={draft}
                onChange={(event) => changeDraft(event.target.value)}
                placeholder={selected.kind === "endpoint" ? "模型名称，例如 grok-3-mini" : "模型名称，例如 deepseek-reasoner"}
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

const CHAT_ENDPOINT_PRESETS: Array<{ id: string; label: string; url: string; model: string }> = [
  { id: "openai", label: "OpenAI", url: "https://api.openai.com/v1", model: "gpt-4o-mini" },
  { id: "openrouter", label: "OpenRouter", url: "https://openrouter.ai/api/v1", model: "" },
  { id: "xai", label: "xAI", url: "https://api.x.ai/v1", model: "grok-3-mini" },
  { id: "siliconflow", label: "SiliconFlow", url: "https://api.siliconflow.cn/v1", model: "" },
  { id: "custom", label: "自定义", url: "", model: "" },
];

function ChatEndpointPanel({
  endpoints,
  onAdded,
}: {
  endpoints: RadarChatEndpoint[];
  onAdded: (sourceId: string, model: string) => void;
}) {
  const queryClient = useQueryClient();
  const [open, setOpen] = useState(false);
  const [presetId, setPresetId] = useState("openai");
  const [name, setName] = useState("OpenAI");
  const [url, setUrl] = useState("https://api.openai.com/v1");
  const [secret, setSecret] = useState("");
  const [model, setModel] = useState("gpt-4o-mini");
  const [verifiedKey, setVerifiedKey] = useState<string | null>(null);
  const [feedback, setFeedback] = useState<{ kind: "ok" | "error"; text: string } | null>(null);

  const applyPreset = (id: string) => {
    const preset = CHAT_ENDPOINT_PRESETS.find((item) => item.id === id);
    setPresetId(id);
    if (!preset) return;
    setUrl(preset.url);
    if (preset.model) setModel(preset.model);
    if (id !== "custom") setName(preset.label);
    setVerifiedKey(null);
    setFeedback(null);
  };

  const draftKey = `${url.trim()}|${secret.trim()}|${model.trim()}`;
  const canTest = Boolean(url.trim() && secret.trim() && model.trim());

  const testMutation = useMutation({
    mutationFn: () =>
      testRadarChatEndpoint({ apiBaseUrl: url.trim(), secret: secret.trim(), model: model.trim() }),
    onSuccess: () => {
      setVerifiedKey(draftKey);
      setFeedback({ kind: "ok", text: "连接成功，可以保存。" });
    },
    onError: (error) => {
      setVerifiedKey(null);
      setFeedback({ kind: "error", text: ipcErrorMessage(error, "测试失败") });
    },
  });
  const saveMutation = useMutation({
    mutationFn: () =>
      saveRadarChatEndpoint({
        displayName: name.trim() || "对话接入",
        apiBaseUrl: url.trim(),
        secret: secret.trim(),
        model: model.trim(),
      }),
    onSuccess: (snapshot) => {
      queryClient.setQueryData(RADAR_SNAPSHOT_QUERY_KEY, snapshot);
      const saved = snapshot.chatEndpoints.at(-1);
      const savedModel = saved?.models[0] ?? model.trim();
      if (saved) onAdded(saved.sourceId, savedModel);
      setSecret("");
      setVerifiedKey(null);
      setFeedback({ kind: "ok", text: `已保存 ${name.trim() || "对话接入"} · ${savedModel}` });
    },
    onError: (error) => setFeedback({ kind: "error", text: ipcErrorMessage(error, "保存失败") }),
  });
  const deleteMutation = useMutation({
    mutationFn: deleteRadarChatEndpoint,
    onSuccess: (snapshot) => queryClient.setQueryData(RADAR_SNAPSHOT_QUERY_KEY, snapshot),
    onError: (error) => setFeedback({ kind: "error", text: ipcErrorMessage(error, "删除失败") }),
  });

  return (
    <div className="flex flex-col gap-2">
      <button
        type="button"
        className="self-start cursor-pointer text-xs font-medium text-q-primary hover:underline"
        onClick={() => setOpen((value) => !value)}
      >
        {open ? "收起其他对话接入" : "添加其他对话接入"}
      </button>
      {open ? (
        <div className="flex flex-col gap-2 rounded-q-control border border-q-border bg-q-surface px-3 py-2.5">
          <p className="text-xs leading-relaxed text-q-text-muted">
            添加 OpenAI 兼容的对话接口（OpenAI / OpenRouter / xAI / SiliconFlow 或自填地址）。密钥只保存在本机凭据库，不进入平台中心。本机 Codex / Grok CLI 登录不能用于分析。
          </p>
          <div className="flex flex-wrap gap-1.5">
            {CHAT_ENDPOINT_PRESETS.map((preset) => (
              <button
                key={preset.id}
                type="button"
                aria-pressed={presetId === preset.id}
                onClick={() => applyPreset(preset.id)}
                className={cn(
                  "cursor-pointer rounded-q-pill px-2.5 py-1 text-[11px] font-medium",
                  presetId === preset.id
                    ? "bg-[var(--q-chip-active-bg)] text-[var(--q-chip-active-text)]"
                    : "border border-q-border text-q-text-secondary hover:text-q-text-primary",
                )}
              >
                {preset.label}
              </button>
            ))}
          </div>
          <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
            <input
              value={name}
              onChange={(event) => setName(event.target.value)}
              placeholder="显示名称"
              aria-label="对话接入显示名称"
              className="h-9 rounded-md border border-q-border bg-q-surface px-2.5 text-xs text-q-text-primary outline-none focus:border-q-primary"
            />
            <input
              value={model}
              onChange={(event) => {
                setModel(event.target.value);
                setVerifiedKey(null);
              }}
              placeholder="模型名"
              aria-label="对话接入模型名"
              className="h-9 rounded-md border border-q-border bg-q-surface px-2.5 text-xs text-q-text-primary outline-none focus:border-q-primary"
            />
          </div>
          <input
            value={url}
            onChange={(event) => {
              setUrl(event.target.value);
              setVerifiedKey(null);
            }}
            placeholder="https://api.example.com/v1"
            aria-label="对话接入请求地址"
            className="h-9 rounded-md border border-q-border bg-q-surface px-2.5 text-xs text-q-text-primary outline-none focus:border-q-primary"
          />
          <input
            type="password"
            value={secret}
            onChange={(event) => {
              setSecret(event.target.value);
              setVerifiedKey(null);
            }}
            placeholder="API Key"
            aria-label="对话接入 API Key"
            autoComplete="off"
            className="h-9 rounded-md border border-q-border bg-q-surface px-2.5 text-xs text-q-text-primary outline-none focus:border-q-primary"
          />
          <div className="flex flex-wrap items-center gap-2">
            <Button
              variant="secondary"
              size="sm"
              disabled={!canTest || testMutation.isPending}
              onClick={() => testMutation.mutate()}
            >
              {testMutation.isPending ? "验证中…" : verifiedKey === draftKey ? "重新验证" : "验证连接"}
            </Button>
            <Button
              size="sm"
              disabled={!canTest || verifiedKey !== draftKey || saveMutation.isPending}
              onClick={() => saveMutation.mutate()}
            >
              {saveMutation.isPending ? "保存中…" : "保存接入"}
            </Button>
          </div>
          {feedback ? (
            <p className={cn("text-xs leading-relaxed", feedback.kind === "ok" ? "text-q-success" : "text-q-danger")}>
              {feedback.text}
            </p>
          ) : (
            <p className="text-xs leading-relaxed text-q-text-muted">验证会发送一次极小请求，可能消耗少量额度。地址必须是 https。</p>
          )}
          {endpoints.length > 0 ? (
            <ul className="flex flex-col gap-1.5">
              {endpoints.map((item) => (
                <li
                  key={item.id}
                  className="flex min-w-0 items-start justify-between gap-2 rounded-q-control border border-q-border bg-q-surface-strong px-2.5 py-1.5"
                >
                  <div className="min-w-0">
                    <p className="truncate text-[12px] font-medium text-q-text-primary">{item.displayName}</p>
                    <p className="truncate text-[11px] text-q-text-muted" title={item.apiBaseUrl}>
                      {item.models.join(" · ") || "未添加模型"}
                    </p>
                  </div>
                  <button
                    type="button"
                    className="shrink-0 cursor-pointer text-[11px] text-q-text-muted hover:text-q-danger"
                    disabled={deleteMutation.isPending}
                    onClick={() => deleteMutation.mutate(item.id)}
                  >
                    删除
                  </button>
                </li>
              ))}
            </ul>
          ) : null}
        </div>
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
      <p className="text-[13px] text-q-text-secondary">
        {title} · {posts.length} 条
        <span className="ml-1.5 text-[12px] text-q-text-secondary">{subtitle}</span>
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

/** 相对天数档：`Nd` 表示过去 N 天（浏览筛选）。 */
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

/** 浏览范围换算成 [起始毫秒, 结束毫秒]；`all` 不限。 */
function browseRangeBounds(rangeKey: string): { start: number; end: number } {
  const custom = parseCustomRange(rangeKey);
  if (custom) {
    return {
      start: Date.parse(`${custom.start}T00:00:00`),
      end: Date.parse(`${custom.end}T23:59:59.999`),
    };
  }
  if (rangeKey === "today") {
    const start = new Date();
    start.setHours(0, 0, 0, 0);
    return { start: start.getTime(), end: Number.MAX_SAFE_INTEGER };
  }
  const days = parseRangeDays(rangeKey) ?? 7;
  return { start: Date.now() - days * 24 * 60 * 60 * 1000, end: Number.MAX_SAFE_INTEGER };
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

/** 时间范围快捷档与筛选 Chip 的统一样式：两主题各由 Chip Token 驱动。 */
function rangeChipClass(active: boolean): string {
  return cn(
    "inline-flex cursor-pointer items-center whitespace-nowrap rounded-q-pill border px-3 py-1.5 text-[12px] font-medium transition-colors duration-150",
    active
      ? "border-[var(--q-chip-active-border)] bg-[var(--q-chip-active-bg)] text-[var(--q-chip-active-text)]"
      : "border-[var(--q-chip-border)] bg-[var(--q-chip-bg)] text-[var(--q-chip-text)] hover:border-[var(--q-chip-active-border)] hover:text-[var(--q-chip-active-text)]",
  );
}
