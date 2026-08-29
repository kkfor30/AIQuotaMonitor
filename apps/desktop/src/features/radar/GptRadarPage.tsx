import { useEffect, useMemo, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  BrainCircuit,
  ExternalLink,
  Heart,
  HelpCircle,
  Link2,
  Megaphone,
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
import { fetchRadarSnapshot, ipcErrorMessage, openExternalUrl, runRadarCheck, saveRadarAnalysisPrefs } from "@/lib/ipc";
import { RADAR_SNAPSHOT_QUERY_KEY } from "@/lib/query-client";
import type { RadarPost } from "@/lib/ipc";
import { RadarConfidenceBadge } from "@/features/radar/RadarConfidenceBadge";

export function GptRadarPage() {
  const queryClient = useQueryClient();
  const [tab, setTab] = useState<RadarTabId>("signal");
  const [filter, setFilter] = useState<"all" | "related" | "none">("all");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [rangeKey, setRangeKey] = useState("3d");
  const [analyze, setAnalyze] = useState(false);
  const [sourceId, setSourceId] = useState<string>("");
  const [userPrompt, setUserPrompt] = useState("");

  const { data, isLoading } = useQuery({
    queryKey: RADAR_SNAPSHOT_QUERY_KEY,
    queryFn: fetchRadarSnapshot,
  });
  const prefsReady = useRef(false);

  useEffect(() => {
    if (!data || prefsReady.current) return;
    prefsReady.current = true;
    setAnalyze(data.analysisPrefs.analyze);
    setRangeKey(data.analysisPrefs.rangeKey || "3d");
    if (data.analysisPrefs.sourceId) setSourceId(data.analysisPrefs.sourceId);
    setUserPrompt(data.analysisPrefs.userPrompt ?? data.analysisPrefs.defaultUserPrompt ?? "");
  }, [data]);

  useEffect(() => {
    if (!prefsReady.current) return;
    const timer = window.setTimeout(() => {
      void saveRadarAnalysisPrefs({
        analyze,
        rangeKey,
        sourceId: sourceId || null,
        userPrompt,
      });
    }, 200);
    return () => window.clearTimeout(timer);
  }, [analyze, rangeKey, sourceId, userPrompt]);

  const checkMutation = useMutation({
    mutationFn: () =>
      runRadarCheck({
        analyze,
        rangeKey,
        sourceId: sourceId || data?.models.find((item) => item.ready)?.sourceId || null,
        model: data?.models.find((item) => item.sourceId === sourceId)?.model ?? null,
        userPrompt,
      }),
    onSuccess: (snapshot) => {
      queryClient.setQueryData(RADAR_SNAPSHOT_QUERY_KEY, snapshot);
    },
  });

  const posts = data?.posts ?? [];
  const visible = posts.filter((post) => {
    if (filter === "all") return true;
    if (filter === "related") return post.filter === "related" || post.filter === "signal";
    return post.filter === "none";
  });
  const selected = visible.find((post) => post.id === selectedId) ?? visible[0] ?? null;
  const models = data?.models ?? [];

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-4 pt-2 pr-2">
      {/* 页头：图标磁贴 + 标题 + 推测声明 + 立即检查 */}
      <header className="glass-panel flex flex-wrap items-start justify-between gap-3 px-5 py-4">
        <div className="flex min-w-0 items-start gap-3.5">
          <span
            aria-hidden
            className="flex h-11 w-11 shrink-0 items-center justify-center rounded-[14px] border border-q-border bg-q-surface-strong text-q-primary shadow-q-sm"
          >
            <Radar size={22} aria-hidden />
          </span>
          <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-2.5">
              <h1 className="text-[20px] font-semibold tracking-tight text-q-text-primary">GPT 重置雷达</h1>
              <span className="rounded-q-pill bg-q-neutral-soft px-2.5 py-1 text-xs text-q-neutral">
                仅为推测，不代表官方结论
              </span>
            </div>
            <p className="mt-1 max-w-2xl text-[13px] leading-relaxed text-q-text-secondary">
              手动同步 CodexRadar 公开首页的 Tibo 动态与中文翻译。我们自己的 AI 分析默认关闭，且只使用英文原文。
            </p>
          </div>
        </div>
        <Button onClick={() => checkMutation.mutate()} disabled={checkMutation.isPending}>
          <RefreshCw size={15} aria-hidden className={checkMutation.isPending ? "animate-spin" : ""} />
          {checkMutation.isPending ? "检查中…" : "立即检查"}
        </Button>
      </header>
      {checkMutation.error && (
        <p className="rounded-q-control border border-q-danger/25 bg-q-danger-soft px-3 py-2 text-xs text-q-danger">
          {ipcErrorMessage(checkMutation.error, "检查失败")}
        </p>
      )}

      {/* 分段 Tab（胶囊分段控件） */}
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
              "data-[active=true]:bg-white data-[active=true]:text-q-primary data-[active=true]:shadow-q-sm",
            )}
          >
            {item.label}
          </button>
        ))}
      </div>

      {isLoading && <p className="text-sm text-q-text-muted">正在加载雷达数据…</p>}
      {tab === "signal" && <SignalSummaryView data={data} />}
      {tab === "tibo" && (
        <TiboFeedView
          posts={visible}
          totalCount={posts.length}
          relatedCount={posts.filter((p) => p.filter === "related" || p.filter === "signal").length}
          noneCount={posts.filter((p) => p.filter === "none").length}
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
          rangeKey={rangeKey}
          sourceId={sourceId || models.find((item) => item.ready)?.sourceId || ""}
          models={models}
          onAnalyzeChange={setAnalyze}
          onRangeChange={setRangeKey}
          onSourceChange={setSourceId}
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

function formatTime(value: number) {
  return new Date(value).toLocaleString("zh-CN", { hour12: false });
}

function sourceLabel(status?: string) {
  if (status === "fresh") return "正常";
  if (status === "stale") return "缓存";
  return "未同步";
}

function sourceTone(status?: string): "success" | "warning" | "neutral" {
  if (status === "fresh") return "success";
  if (status === "stale") return "warning";
  return "neutral";
}

/** 帖子信号徽章色调：显式重置 → 红；信号/相关 → 橙；无信号 → 灰 */
function postBadgeTone(post: RadarPost): "danger" | "warning" | "neutral" | "primary" {
  if (post.explicitReset) return "danger";
  if (post.filter === "signal" || post.filter === "related") return "warning";
  if (post.filter === "none") return "neutral";
  return "primary";
}

/* ————————————————— 信号摘要（设计稿 04） ————————————————— */

function SignalSummaryView({ data }: { data: Awaited<ReturnType<typeof fetchRadarSnapshot>> | undefined }) {
  const latest = data?.latest;
  const notice = data?.notice;

  return (
    <div className="flex flex-col gap-4">
      <div className="grid grid-cols-1 gap-4 xl:grid-cols-3">
        {/* CodexRadar 来源 */}
        <section className="glass-panel flex flex-col gap-3 p-4">
          <div className="flex items-center gap-2.5">
            <span
              aria-hidden
              className="flex h-9 w-9 shrink-0 items-center justify-center rounded-[11px] border border-q-border bg-q-surface-strong text-q-primary shadow-q-sm"
            >
              <Radar size={17} aria-hidden />
            </span>
            <h2 className="text-[14px] font-semibold tracking-tight text-q-text-primary">CodexRadar 来源</h2>
          </div>
          <div className="flex items-center gap-2">
            <StatusBadge tone={sourceTone(data?.sourceStatus)}>{sourceLabel(data?.sourceStatus)}</StatusBadge>
            <span className="text-[11px] text-q-text-muted">
              {data?.lastSyncedAt ? `上次同步 ${formatTime(data.lastSyncedAt)}` : "尚未同步"}
            </span>
          </div>
          <p className="text-xs leading-relaxed text-q-text-secondary">
            内容来自 codexradar.com 公开首页转载的 Tibo 原文，同步失败时保留最后成功快照。
          </p>
          <Button
            variant="ghost"
            size="sm"
            className="mt-auto self-start"
            onClick={() => void openExternalUrl("https://codexradar.com/")}
          >
            <ExternalLink size={14} aria-hidden />
            打开 CodexRadar
          </Button>
        </section>

        {/* 顶部公告 */}
        <section className="glass-panel flex flex-col gap-3 p-4">
          <div className="flex items-center gap-2.5">
            <span
              aria-hidden
              className="flex h-9 w-9 shrink-0 items-center justify-center rounded-[11px] border border-q-border bg-q-surface-strong text-q-warning shadow-q-sm"
            >
              <Megaphone size={17} aria-hidden />
            </span>
            <h2 className="text-[14px] font-semibold tracking-tight text-q-text-primary">顶部公告</h2>
          </div>
          {notice ? (
            <div className="flex min-h-0 flex-col gap-1.5">
              <p className="text-[13px] font-semibold leading-relaxed text-q-text-primary">{notice.headline}</p>
              {notice.lead && <p className="text-xs leading-relaxed text-q-text-secondary">{notice.lead}</p>}
              {notice.items[0] && <p className="text-xs leading-relaxed text-q-text-muted">{notice.items[0]}</p>}
            </div>
          ) : latest ? (
            <div className="flex min-h-0 flex-col gap-1.5">
              <StatusBadge tone={postBadgeTone(latest)}>{latest.badge}</StatusBadge>
              <p className="line-clamp-3 text-[13px] leading-relaxed text-q-text-primary">
                {latest.summary ?? latest.translatedText ?? latest.text}
              </p>
              <p className="text-[11px] text-q-text-muted">{formatTime(latest.postedAt)}</p>
            </div>
          ) : (
            <p className="text-xs text-q-text-muted">暂无公告</p>
          )}
        </section>

        {/* AI 辅助结论 */}
        <section className="glass-panel flex flex-col gap-3 p-4">
          <div className="flex items-center gap-2.5">
            <span
              aria-hidden
              className="flex h-9 w-9 shrink-0 items-center justify-center rounded-[11px] border border-q-border bg-q-surface-strong text-q-primary shadow-q-sm"
            >
              <BrainCircuit size={17} aria-hidden />
            </span>
            <h2 className="text-[14px] font-semibold tracking-tight text-q-text-primary">AI 辅助结论</h2>
          </div>
          {data?.analysis?.errorMessage ? (
            <p className="text-xs leading-relaxed text-q-danger">{data.analysis.errorMessage}</p>
          ) : null}
          {data?.analysis?.conclusion ? (
            <div className="flex min-h-0 flex-col gap-2">
              <div className="flex flex-col gap-1">
                <p className="text-[11px] font-medium text-q-text-muted">结论</p>
                <p className="text-[13px] leading-relaxed text-q-text-primary" data-selectable="true">
                  {data.analysis.conclusion}
                </p>
              </div>
              {data.analysis.analysisBasis ? (
                <div className="flex flex-col gap-1">
                  <p className="text-[11px] font-medium text-q-text-muted">
                    分析依据 · {data.analysis.analysisBasis.length} 字
                  </p>
                  <div
                    className="max-h-44 overflow-y-auto rounded-q-card border border-q-border bg-q-primary-softer px-3 py-2.5 text-xs leading-relaxed text-q-text-secondary"
                    data-selectable="true"
                  >
                    {data.analysis.analysisBasis}
                  </div>
                </div>
              ) : null}
              <div className="mt-auto flex flex-wrap items-center gap-2">
                <RadarConfidenceBadge confidence={data.analysis.confidence} />
                {data.analysis.model && (
                  <span className="text-[11px] text-q-text-muted">{data.analysis.model}</span>
                )}
              </div>
            </div>
          ) : (
            <div className="flex flex-col gap-2">
              <p className="text-xs leading-relaxed text-q-text-muted">
                {data?.analysis?.errorMessage
                  ? "本次分析失败，可更换模型后重试。"
                  : "未分析。可在 AI 辅助分析 Tab 开启后随立即检查运行。"}
              </p>
              <RadarConfidenceBadge confidence={data?.analysis?.confidence ?? null} />
            </div>
          )}
        </section>
      </div>

      {/* 检查历史 */}
      <section className="glass-panel flex flex-col gap-2.5 p-4">
        <h2 className="text-[14px] font-semibold tracking-tight text-q-text-primary">检查历史</h2>
        {(data?.checks ?? []).length === 0 && <p className="text-xs text-q-text-muted">还没有检查记录</p>}
        <div className="flex flex-col">
          {(data?.checks ?? []).map((check) => (
            <div
              key={check.id}
              className="flex flex-wrap items-center gap-x-4 gap-y-1 border-b border-q-border/60 py-2 text-xs last:border-b-0"
            >
              <span className="w-[150px] shrink-0 tabular-nums text-q-text-secondary">
                {formatTime(check.startedAt)}
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
                      : "失败"}
              </StatusBadge>
              <span className="shrink-0 text-q-text-muted">{check.postCount} 条</span>
              {check.errorMessage && (
                <span className="min-w-0 flex-1 truncate text-q-danger" title={check.errorMessage}>
                  {check.errorMessage}
                </span>
              )}
            </div>
          ))}
        </div>
      </section>

      <p className="flex items-center gap-1.5 px-1 text-[11px] text-q-text-muted">
        <ShieldCheck size={13} aria-hidden className="shrink-0" />
        雷达内容独立于平台额度状态；AI 分析只接收英文原文、发布时间与原帖链接，不发送凭据。
      </p>
    </div>
  );
}

/* ————————————————— Tibo 动态/* ————————————————— Tibo 动态（设计稿 05） ————————————————— */

function TiboFeedView({
  posts,
  totalCount,
  relatedCount,
  noneCount,
  selected,
  filter,
  onFilter,
  onSelect,
}: {
  posts: RadarPost[];
  totalCount: number;
  relatedCount: number;
  noneCount: number;
  selected: RadarPost | null;
  filter: "all" | "related" | "none";
  onFilter: (value: "all" | "related" | "none") => void;
  onSelect: (id: string) => void;
}) {
  const chips: Array<{ id: "all" | "related" | "none"; label: string; count: number }> = [
    { id: "all", label: "全部", count: totalCount },
    { id: "related", label: "重置相关", count: relatedCount },
    { id: "none", label: "无重置信号", count: noneCount },
  ];

  return (
    <div className="flex flex-col gap-3">
      {/* 筛选 chips：带计数 */}
      <div className="flex flex-wrap items-center gap-2 px-1">
        {chips.map((chip) => (
          <button
            key={chip.id}
            type="button"
            onClick={() => onFilter(chip.id)}
            aria-pressed={filter === chip.id}
            className={cn(
              "inline-flex cursor-pointer items-center gap-1.5 rounded-q-pill px-3 py-1.5 text-[12px] font-medium transition-colors duration-150",
              filter === chip.id
                ? "bg-q-primary text-white shadow-[0_4px_12px_rgba(10,102,255,0.28)]"
                : "border border-q-border bg-white/70 text-q-text-secondary shadow-q-sm hover:border-q-border-selected hover:text-q-primary",
            )}
          >
            {chip.label}
            <span
              className={cn(
                "rounded-full px-1.5 text-[10px] tabular-nums",
                filter === chip.id ? "bg-white/25" : "bg-q-primary-soft text-q-primary",
              )}
            >
              {chip.count}
            </span>
          </button>
        ))}
      </div>

      <div className="grid min-h-[420px] max-h-[calc(100vh-264px)] grid-cols-1 gap-4 xl:grid-cols-[minmax(0,0.85fr)_minmax(0,1.4fr)]">
        {/* 动态列表：视口相关限高，超出在自身窗口内滚动，不把页面无限撑长 */}
        <section className="glass-panel flex min-h-0 flex-col gap-2.5 p-3.5">
          <div className="min-h-0 flex-1 space-y-2 overflow-y-auto pr-0.5">
            {posts.length === 0 && (
              <p className="px-1 py-6 text-center text-xs text-q-text-muted">点「立即检查」同步 CodexRadar。</p>
            )}
            {posts.map((post) => (
              <button
                key={post.id}
                type="button"
                onClick={() => onSelect(post.id)}
                aria-current={selected?.id === post.id ? "true" : undefined}
                className={cn(
                  "w-full cursor-pointer rounded-[14px] border p-3.5 text-left transition-colors duration-150",
                  selected?.id === post.id
                    ? "border-q-border-selected bg-q-primary-softer shadow-q-sm"
                    : "border-q-border bg-q-surface-strong hover:border-q-border-selected",
                )}
              >
                <div className="flex items-center gap-2">
                  <StatusBadge tone={postBadgeTone(post)}>{post.badge}</StatusBadge>
                  <span className="ml-auto shrink-0 text-[11px] tabular-nums text-q-text-muted">
                    {formatTime(post.postedAt)}
                  </span>
                </div>
                <p className="mt-2 line-clamp-3 text-[13px] leading-relaxed text-q-text-primary">
                  {post.summary ?? post.translatedText ?? post.text}
                </p>
                <div className="mt-2.5 flex items-center gap-3.5 text-[11px] text-q-text-muted">
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

        {/* 动态详情 */}
        <section className="glass-panel flex min-h-0 flex-col gap-3.5 overflow-y-auto p-4">
          {selected ? (
            <>
              <div className="flex flex-wrap items-center gap-2.5">
                <StatusBadge tone={postBadgeTone(selected)}>{selected.badge}</StatusBadge>
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
                  className="max-h-56 overflow-y-auto whitespace-pre-wrap rounded-q-control border border-q-border bg-q-surface-muted px-3.5 py-3 text-[13px] leading-relaxed text-q-text-secondary"
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
            <p className="text-xs text-q-text-muted">从左侧选择一条动态</p>
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
  rangeKey,
  sourceId,
  models,
  userPrompt,
  defaultUserPrompt,
  onAnalyzeChange,
  onRangeChange,
  onSourceChange,
  onUserPromptChange,
}: {
  data: Awaited<ReturnType<typeof fetchRadarSnapshot>> | undefined;
  analyze: boolean;
  rangeKey: string;
  sourceId: string;
  models: { sourceId: string; displayName: string; model: string; ready: boolean }[];
  userPrompt: string;
  defaultUserPrompt: string;
  onAnalyzeChange: (value: boolean) => void;
  onRangeChange: (value: string) => void;
  onSourceChange: (value: string) => void;
  onUserPromptChange: (value: string) => void;
}) {
  const analysis = data?.analysis;
  const latestCheck = data?.checks[0];
  const analyzeError =
    analysis?.errorMessage ||
    (latestCheck?.analyzeStatus === "failed" ? latestCheck.errorMessage : null) ||
    null;
  const analysisInput = useMemo(() => {
    const { start, end } = rangeBoundsMs(rangeKey);
    return (data?.posts ?? []).filter((post) => post.postedAt >= start && post.postedAt <= end);
  }, [data?.posts, rangeKey]);
  const customRange = customRangeOf(rangeKey);
  const customActive = !QUICK_RANGES.some((range) => range.id === rangeKey) && customRange !== null;
  const todayIso = toIsoDate(new Date());
  const applyCustomRange = (start: string, end: string) => {
    if (!start || !end || start.length !== 10 || end.length !== 10) return;
    const [from, to] = start <= end ? [start, end] : [end, start];
    onRangeChange(`range:${from}:${to}`);
  };
  const dateInputClass =
    "h-9 rounded-md border border-q-border bg-q-surface px-2.5 text-xs text-q-text-primary outline-none focus:border-q-primary";
  const selectClass =
    "h-10 w-full cursor-pointer rounded-q-control border border-q-border bg-q-surface-strong px-3 text-sm text-q-text-primary outline-none focus:border-q-primary";

  return (
    <div className="flex flex-col gap-4">
      <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
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
          <div className="flex flex-col gap-1.5 text-sm">
            <span className="font-medium text-q-text-primary">时间范围</span>
            <div className="flex flex-wrap items-center gap-2">
              {QUICK_RANGES.map((range) => (
                <button
                  key={range.id}
                  type="button"
                  aria-pressed={rangeKey === range.id}
                  onClick={() => onRangeChange(range.id)}
                  className={rangeChipClass(rangeKey === range.id)}
                >
                  {range.label}
                </button>
              ))}
              <button
                type="button"
                aria-pressed={customActive}
                onClick={() => {
                  if (!customActive) onRangeChange(defaultCustomRangeKey());
                }}
                className={rangeChipClass(customActive)}
              >
                自定义
              </button>
            </div>
            {customActive && customRange && (
              <div className="flex flex-wrap items-center gap-2.5 rounded-q-control border border-q-border bg-q-surface-strong px-3 py-2.5">
                <input
                  type="date"
                  value={customRange.start}
                  max={todayIso}
                  onChange={(event) => applyCustomRange(event.target.value, customRange.end)}
                  aria-label="分析开始日期"
                  className={dateInputClass}
                />
                <span className="text-xs text-q-text-muted">至</span>
                <input
                  type="date"
                  value={customRange.end}
                  max={todayIso}
                  onChange={(event) => applyCustomRange(customRange.start, event.target.value)}
                  aria-label="分析结束日期"
                  className={dateInputClass}
                />
                <span className="text-xs text-q-text-muted">包含起止两天</span>
              </div>
            )}
          </div>
          <label className="flex flex-col gap-1.5 text-sm">
            <span className="font-medium text-q-text-primary">分析模型</span>
            <select value={sourceId} onChange={(event) => onSourceChange(event.target.value)} className={selectClass}>
              {models.length === 0 && <option value="">请先在平台中心接入 API Key</option>}
              {models.map((item) => (
                <option key={item.sourceId} value={item.sourceId} disabled={!item.ready}>
                  {item.displayName} · {item.model}
                  {item.ready ? "" : "（不可用）"}
                </option>
              ))}
            </select>
          </label>
          <p className="text-xs leading-relaxed text-q-text-muted">
            每次检查都会按当前时间范围重新分析所选帖子，不跳过、不沿用旧结果。
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
          {analysisInput.length === 0 ? (
            <p className="text-xs leading-relaxed text-q-text-muted">
              当前时间窗内没有可分析的帖子。Tibo 近期没有新动态时，可切换「自定义」扩大时间范围后重试。
            </p>
          ) : (
            <div className="flex min-h-0 flex-col gap-2">
              <p className="text-xs text-q-text-secondary">将发送 {analysisInput.length} 条</p>
              <div className="max-h-64 space-y-1.5 overflow-y-auto pr-0.5">
                {analysisInput.slice(0, 8).map((post) => (
                  <div
                    key={post.id}
                    className="rounded-q-control border border-q-border bg-q-surface-strong px-3 py-2 text-[12px] leading-relaxed"
                  >
                    <span className="mr-2 shrink-0 tabular-nums text-q-text-muted">{formatTime(post.postedAt)}</span>
                    <span className="text-q-text-primary">{post.text}</span>
                  </div>
                ))}
                {analysisInput.length > 8 && (
                  <p className="px-1 text-[11px] text-q-text-muted">…共 {analysisInput.length} 条</p>
                )}
              </div>
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
          <h2 className="text-[14px] font-semibold tracking-tight text-q-text-primary">辅助结论</h2>
          <div className="flex flex-wrap items-center gap-2">
            <RadarConfidenceBadge confidence={analysis?.confidence ?? null} />
            {analysis?.model ? <span className="text-[11px] text-q-text-muted">{analysis.model}</span> : null}
          </div>
        </div>
        {analyzeError ? <p className="text-xs leading-relaxed text-q-danger">{analyzeError}</p> : null}
        {analysis?.conclusion ? (
          <>
            <div className="flex flex-col gap-1.5">
              <p className="text-[11px] font-medium text-q-text-muted">结论</p>
              <p className="text-[15px] font-medium leading-relaxed text-q-text-primary" data-selectable="true">
                {analysis.conclusion}
              </p>
            </div>
            {analysis.analysisBasis ? (
              <div className="flex flex-col gap-1.5">
                <p className="text-[11px] font-medium text-q-text-muted">
                  分析依据 · {analysis.analysisBasis.length} 字
                </p>
                <div
                  className="max-h-72 overflow-y-auto rounded-q-card border border-q-border bg-q-primary-softer px-3.5 py-3 text-[13px] leading-relaxed text-q-text-secondary"
                  data-selectable="true"
                >
                  {analysis.analysisBasis}
                </div>
              </div>
            ) : null}
            <div className="grid grid-cols-1 gap-x-8 gap-y-3 xl:grid-cols-2">
              <ListBlock title="引用" icon={<Link2 size={13} aria-hidden />} items={analysis.citations} />
              <ListBlock title="支持依据" icon={<ThumbsUp size={13} aria-hidden />} items={analysis.support} />
              <ListBlock title="反向依据" icon={<ThumbsDown size={13} aria-hidden />} items={analysis.against} />
              <ListBlock title="不确定性" icon={<HelpCircle size={13} aria-hidden />} items={analysis.uncertainty} />
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
        <span className="text-q-text-muted">· {items.length}</span>
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

/** 时间范围换算成 [起始毫秒, 结束毫秒]（含端点）；无上限用 MAX_SAFE_INTEGER。 */
function rangeBoundsMs(rangeKey: string): { start: number; end: number } {
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

function rangeChipClass(active: boolean): string {
  return cn(
    "inline-flex cursor-pointer items-center rounded-q-pill px-3 py-1.5 text-[12px] font-medium transition-colors duration-150",
    active
      ? "bg-q-primary text-white shadow-[0_4px_12px_rgba(10,102,255,0.28)]"
      : "border border-q-border bg-white/70 text-q-text-secondary shadow-q-sm hover:border-q-border-selected hover:text-q-primary",
  );
}
