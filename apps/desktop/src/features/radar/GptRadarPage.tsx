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
import {
  addRadarCustomModel,
  cancelRadarCheck,
  confirmRadarQuotaChange,
  deleteRadarCustomModel,
  fetchRadarSnapshot,
  ipcErrorMessage,
  openExternalUrl,
  runRadarCheck,
  saveRadarAnalysisPrefs,
  testRadarModel,
} from "@/lib/ipc";
import { RADAR_SNAPSHOT_QUERY_KEY } from "@/lib/query-client";
import type { RadarModelOption, RadarPost } from "@/lib/ipc";
import { quotaStatusText, radarPhaseLabel } from "@/features/hoverbar/hoverbar-state";
import { useContainerWidth, TIBO_SPLIT_MIN_PX } from "@/lib/use-container-width";
import { ArrowLeft } from "lucide-react";

export function GptRadarPage() {
  const queryClient = useQueryClient();
  const [tab, setTab] = useState<RadarTabId>("signal");
  const [filter, setFilter] = useState<"all" | "related" | "none">("all");
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

  const checkCancelled = checkMutation.error
    ? ipcErrorMessage(checkMutation.error, "检查失败").includes("已终止")
    : false;
  const posts = data?.posts ?? [];
  const visible = posts.filter((post) => {
    if (filter === "all") return true;
    if (filter === "related") return post.filter === "related" || post.filter === "signal";
    return post.filter === "none";
  });
  const selected = visible.find((post) => post.id === selectedId) ?? visible[0] ?? null;

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-hidden p-4 pt-2 pr-2">
      {/* 页头：图标磁贴 + 标题 + 推测声明 + 立即检查 */}
      <header className="glass-panel flex shrink-0 flex-wrap items-start justify-between gap-3 px-5 py-4">
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
        {checkMutation.isPending ? (
          <Button variant="ghost" onClick={() => void cancelRadarCheck()}>
            <RefreshCw size={15} aria-hidden className="animate-spin" />
            终止检查
          </Button>
        ) : (
          <Button onClick={() => checkMutation.mutate()}>
            <RefreshCw size={15} aria-hidden />
            立即检查
          </Button>
        )}
      </header>
      {checkMutation.error && !checkCancelled && (
        <p className="rounded-q-control border border-q-danger/25 bg-q-danger-soft px-3 py-2 text-xs text-q-danger">
          {ipcErrorMessage(checkMutation.error, "检查失败")}
        </p>
      )}

      {/* 分段 Tab（胶囊分段控件） */}
      <div
        role="tablist"
        className="inline-flex w-fit shrink-0 items-center gap-1 rounded-q-pill border border-q-border bg-q-surface-muted p-1"
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

      {isLoading && <p className="shrink-0 text-sm text-q-text-muted">正在加载雷达数据…</p>}
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
          sourceId={sourceId || modelOptions.find((item) => item.ready)?.sourceId || ""}
          modelChoice={modelChoice}
          models={modelOptions}
          onAnalyzeChange={setAnalyze}
          onRangeChange={setRangeKey}
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

function SignalSummaryView({ data }: { data: Awaited<ReturnType<typeof fetchRadarSnapshot>> | undefined }) {
  const queryClient = useQueryClient();
  const latest = data?.latest;
  const event = data?.event ?? null;
  const phase = radarPhaseLabel(event?.phase);
  const source = data?.sourceAssessment;
  const ai = data?.aiAssessment;
  const verifications = data?.quotaVerifications ?? [];
  const timeline = event?.timeline ?? [];
  const aiState = !ai ? "not_analyzed" : ai.enabled ? ai.state : "disabled";
  const confirmMutation = useMutation({
    mutationFn: confirmRadarQuotaChange,
    onSuccess: (snapshot) => queryClient.setQueryData(RADAR_SNAPSHOT_QUERY_KEY, snapshot),
  });
  const sourceHeadline =
    source?.headline ?? data?.notice?.headline ?? latest?.summary ?? latest?.translatedText ?? latest?.text ?? "暂未同步来源内容";

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto pr-1">
      <div className="grid grid-cols-[repeat(auto-fit,minmax(min(100%,320px),1fr))] gap-4">
        {/* 两路判断 · CodexRadar 来源 */}
        <section className="glass-panel flex flex-col gap-3 p-4">
          <div className="flex items-center gap-2.5">
            <span
              aria-hidden
              className="flex h-9 w-9 shrink-0 items-center justify-center rounded-[11px] border border-q-border bg-q-surface-strong text-q-primary shadow-q-sm"
            >
              <Radar size={17} aria-hidden />
            </span>
            <h2 className="text-[14px] font-semibold tracking-tight text-q-text-primary">CodexRadar 来源判断</h2>
          </div>
          <div className="flex items-center gap-2">
            <StatusBadge tone={sourceTone(data?.sourceStatus)}>{sourceLabel(data?.sourceStatus)}</StatusBadge>
            <span className="text-[11px] text-q-text-muted">
              {data?.lastSyncedAt ? `上次同步 ${formatTime(data.lastSyncedAt)}` : "尚未同步"}
            </span>
          </div>
          <p className="text-[13px] font-semibold leading-relaxed text-q-text-primary" data-selectable="true">
            {sourceHeadline}
          </p>
          {source?.lead && <p className="text-xs leading-relaxed text-q-text-secondary">{source.lead}</p>}
          <p className="text-xs leading-relaxed text-q-text-secondary">
            内容来自 codexradar.com 公开首页转载的 Tibo 原文，同步失败时保留最后成功快照；本行不受 AI 开关影响。
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

        {/* 两路判断 · AI 辅助 */}
        <section className="glass-panel flex flex-col gap-3 p-4">
          <div className="flex items-center gap-2.5">
            <span
              aria-hidden
              className="flex h-9 w-9 shrink-0 items-center justify-center rounded-[11px] border border-q-border bg-q-surface-strong text-q-primary shadow-q-sm"
            >
              <BrainCircuit size={17} aria-hidden />
            </span>
            <h2 className="text-[14px] font-semibold tracking-tight text-q-text-primary">AI 辅助判断</h2>
            {ai?.enabled && ai.state === "pending" ? (
              <span className="rounded-q-pill bg-q-warning-soft px-2.5 py-1 text-[11px] text-q-warning">
                有新动态待分析
              </span>
            ) : null}
            {aiState === "disabled" ? (
              <span className="rounded-q-pill bg-q-neutral-soft px-2.5 py-1 text-[11px] text-q-neutral">已关闭</span>
            ) : null}
          </div>
          {aiState === "disabled" ? (
            <div className="flex flex-col gap-1.5">
              <p className="text-[13px] leading-relaxed text-q-text-primary">AI 分析已关闭，来源行与额度验证不受影响。</p>
              <p className="text-[11px] text-q-text-muted">
                {ai?.history
                  ? `最近一次成功分析 ${formatTime(ai.history.createdAt)} · ${ai.history.model ?? "未知模型"}`
                  : "未运行过 AI 分析"}
              </p>
            </div>
          ) : aiState === "failed" ? (
            <div className="flex flex-col gap-1.5">
              <p className="text-xs leading-relaxed text-q-danger">{ai?.latestError ?? "本次分析失败"}</p>
              <p className="text-[11px] text-q-text-muted">
                {ai?.history
                  ? `历史分析保留：${formatTime(ai.history.createdAt)} · ${ai.history.conclusion ?? ""}`
                  : "没有可展示的历史分析"}
              </p>
            </div>
          ) : ai?.current?.conclusion ? (
            <div className="flex min-h-0 flex-col gap-2">
              <div className="flex flex-col gap-1">
                <p className="text-[11px] font-semibold tracking-widest text-q-primary">结论</p>
                <p className="text-[13px] leading-relaxed text-q-text-primary" data-selectable="true">
                  {ai.current.conclusion}
                </p>
              </div>
              {ai.current.analysisBasis ? (
                <div className="flex flex-col gap-1">
                  <p className="text-[11px] font-semibold tracking-widest text-q-primary">
                    分析依据 · {ai.current.analysisBasis.length} 字
                  </p>
                  <div
                    className="max-h-44 overflow-y-auto rounded-q-card border border-q-border bg-q-primary-softer px-3 py-2.5 text-xs leading-relaxed text-q-text-secondary"
                    data-selectable="true"
                  >
                    {ai.current.analysisBasis}
                  </div>
                </div>
              ) : null}
              <div className="mt-auto flex flex-wrap items-center gap-2">
                {ai.current.model && <span className="text-[11px] text-q-text-muted">{ai.current.model}</span>}
                <span className="text-[11px] text-q-text-muted">{formatTime(ai.current.createdAt)} 分析</span>
              </div>
            </div>
          ) : (
            <p className="text-xs leading-relaxed text-q-text-muted">
              未分析。可在 AI 辅助分析 Tab 开启后随立即检查运行。
            </p>
          )}
        </section>
      </div>

      <div className="grid grid-cols-[repeat(auto-fit,minmax(min(100%,320px),1fr))] gap-4">
        {/* 当前重置事件 */}
        <section className="glass-panel flex flex-col gap-3 p-4">
          <div className="flex items-center gap-2.5">
            <span
              aria-hidden
              className="flex h-9 w-9 shrink-0 items-center justify-center rounded-[11px] border border-q-border bg-q-surface-strong text-q-warning shadow-q-sm"
            >
              <Megaphone size={17} aria-hidden />
            </span>
            <h2 className="text-[14px] font-semibold tracking-tight text-q-text-primary">当前重置事件</h2>
            {phase ? (
              <span className="rounded-q-pill bg-q-primary-soft px-2.5 py-1 text-[11px] text-q-primary">{phase}</span>
            ) : null}
          </div>
          {event ? (
            <div className="flex min-h-0 flex-col gap-2">
              <p className="text-[11px] font-semibold tracking-widest text-q-primary">结论</p>
              <p className="text-[13px] font-semibold leading-relaxed text-q-text-primary" data-selectable="true">
                {event.title}
              </p>
              {event.summary && (
                <div className="flex flex-col gap-1">
                  <p className="text-[11px] font-semibold tracking-widest text-q-primary">分析依据</p>
                  <p className="text-xs leading-relaxed text-q-text-secondary" data-selectable="true">
                    {event.summary}
                  </p>
                </div>
              )}
              <p className="text-[11px] text-q-text-muted">
                首次信号 {formatTime(event.firstSignalAt)} · 最新证据 {formatTime(event.latestEvidenceAt)}
              </p>
              {timeline.length > 0 && (
                <div className="flex flex-col gap-1.5 rounded-q-control border border-q-border bg-q-surface-strong px-3 py-2.5">
                  {timeline.map((node) => (
                    <div key={`${node.kind}-${node.at}`} className="flex items-center gap-2 text-xs">
                      <span
                        aria-hidden
                        className={cn(
                          "h-2 w-2 shrink-0 rounded-full",
                          node.kind === "closed" ? "bg-q-neutral" : "bg-q-success",
                        )}
                      />
                      <span className="text-q-text-secondary">{node.label}</span>
                      <span className="ml-auto shrink-0 tabular-nums text-q-text-muted">{formatTime(node.at)}</span>
                    </div>
                  ))}
                </div>
              )}
            </div>
          ) : (
            <p className="text-xs leading-relaxed text-q-text-muted">
              暂无进行中的重置事件。出现强信号并开启 AI 分析后，会自动建立事件并累计证据。
            </p>
          )}
        </section>

        {/* 本机额度验证 */}
        <section className="glass-panel flex flex-col gap-3 p-4">
          <div className="flex items-center gap-2.5">
            <span
              aria-hidden
              className="flex h-9 w-9 shrink-0 items-center justify-center rounded-[11px] border border-q-border bg-q-surface-strong text-q-success shadow-q-sm"
            >
              <ShieldCheck size={17} aria-hidden />
            </span>
            <h2 className="text-[14px] font-semibold tracking-tight text-q-text-primary">本机额度验证</h2>
          </div>
          {verifications.length === 0 ? (
            <p className="text-xs text-q-text-muted">未接入 GPT 额度来源。</p>
          ) : (
            <div className="flex flex-col gap-2.5">
              {verifications.map((item) => (
                <div
                  key={item.sourceId}
                  className="flex flex-col gap-1.5 rounded-q-control border border-q-border bg-q-surface-strong px-3 py-2.5"
                >
                  <div className="flex items-center gap-2">
                    <b className="text-[13px] text-q-text-primary">{item.accountName}</b>
                    <StatusBadge tone={quotaTone(item.status)}>{quotaStatusText(item.status)}</StatusBadge>
                    {item.attribution === "user_confirmed" ? (
                      <span className="text-[11px] text-q-text-muted">用户已确认</span>
                    ) : null}
                    {item.attribution === "radar_correlated" ? (
                      <span className="text-[11px] text-q-text-muted">与事件时间相关</span>
                    ) : null}
                  </div>
                  {item.status === "unavailable" && (
                    <p className="text-[11px] leading-relaxed text-q-text-muted">
                      当前网络无法获取 Codex 额度，不影响来源与 AI 判断。
                    </p>
                  )}
                  {item.note && <p className="text-[11px] text-q-text-muted">{item.note}</p>}
                  <p className="text-[11px] text-q-text-muted">
                    {item.windowLabel ? `${item.windowLabel} · ` : ""}
                    {item.lastSuccessAt ? `上次成功 ${formatTime(item.lastSuccessAt)}` : "尚无成功快照"}
                    {item.previous?.remaining != null && item.current?.remaining != null
                      ? ` · 剩余 ${Math.round(item.previous.remaining * 100)}% → ${Math.round(item.current.remaining * 100)}%`
                      : ""}
                    {item.current?.resetAt ? ` · 原定重置 ${formatTime(item.current.resetAt)}` : ""}
                  </p>
                  {["unscheduled_reset", "possible_reset"].includes(item.status) &&
                    item.current &&
                    item.attribution !== "user_confirmed" && (
                      <Button
                        variant="ghost"
                        size="sm"
                        className="self-start"
                        disabled={confirmMutation.isPending}
                        onClick={() =>
                          confirmMutation.mutate({
                            accountId: item.accountId,
                            sourceId: item.sourceId,
                            capturedAt: item.current?.capturedAt ?? 0,
                          })
                        }
                      >
                        确认这是我手动使用的重置卡
                      </Button>
                    )}
                </div>
              ))}
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
      <div className="flex shrink-0 flex-wrap items-center gap-2 px-1">
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
              className="flex w-fit cursor-pointer items-center gap-1.5 self-start rounded-q-pill border border-q-border bg-white/70 px-3 py-1.5 text-[12px] font-medium text-q-text-secondary shadow-q-sm transition-colors hover:border-q-border-selected hover:text-q-primary"
            >
              <ArrowLeft size={13} aria-hidden />
              返回动态列表
            </button>
          )}
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
  rangeKey,
  sourceId,
  modelChoice,
  models,
  userPrompt,
  defaultUserPrompt,
  onAnalyzeChange,
  onRangeChange,
  onSourceChange,
  onModelChange,
  onUserPromptChange,
}: {
  data: Awaited<ReturnType<typeof fetchRadarSnapshot>> | undefined;
  analyze: boolean;
  rangeKey: string;
  sourceId: string;
  modelChoice: string;
  models: RadarModelOption[];
  userPrompt: string;
  defaultUserPrompt: string;
  onAnalyzeChange: (value: boolean) => void;
  onRangeChange: (value: string) => void;
  onSourceChange: (value: string) => void;
  onModelChange: (value: string) => void;
  onUserPromptChange: (value: string) => void;
}) {
  const analysis = data?.analysis;
  const latestCheck = data?.checks[0];
  const analyzeError = latestCheck?.analyzeStatus === "failed" ? latestCheck.errorMessage : null;
  const analysisInput = useMemo(() => {
    const { start, end } = rangeBoundsMs(rangeKey);
    return (data?.posts ?? []).filter((post) => post.postedAt >= start && post.postedAt <= end);
  }, [data?.posts, rangeKey]);
  // 事件上下文 = 当前事件已关联、但不在本次新增范围内的旧原帖（与后端提示词分组一致）。
  const contextInput = useMemo(() => {
    const eventIds = new Set(data?.event?.postIds ?? []);
    const deltaIds = new Set(analysisInput.map((post) => post.id));
    return (data?.posts ?? []).filter((post) => eventIds.has(post.id) && !deltaIds.has(post.id));
  }, [data?.posts, data?.event?.postIds, analysisInput]);
  const customRange = customRangeOf(rangeKey);
  const customActive = !QUICK_RANGES.some((range) => range.id === rangeKey) && customRange !== null;
  const todayIso = toIsoDate(new Date());
  // 所选来源 + 模型的选项；modelChoice 失配（来源切换、旧偏好）时回退该来源默认模型。
  const selectedModelOption =
    models.find((item) => item.sourceId === sourceId && item.model === modelChoice) ??
    models.find((item) => item.sourceId === sourceId) ??
    null;
  // 记住最近一次自定义区间：切到快捷档再切回「自定义」时恢复，而不是重置成默认 30 天
  const lastCustomRangeRef = useRef<string | null>(
    customRange ? `range:${customRange.start}:${customRange.end}` : null,
  );
  const applyCustomRange = (start: string, end: string) => {
    if (!start || !end || start.length !== 10 || end.length !== 10) return;
    const [from, to] = start <= end ? [start, end] : [end, start];
    lastCustomRangeRef.current = `range:${from}:${to}`;
    onRangeChange(`range:${from}:${to}`);
  };
  const switchToCustom = () => {
    onRangeChange(lastCustomRangeRef.current ?? defaultCustomRangeKey());
  };
  const dateInputClass =
    "h-9 rounded-md border border-q-border bg-q-surface px-2.5 text-xs text-q-text-primary outline-none focus:border-q-primary";
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
                  if (!customActive) switchToCustom();
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
                  {item.displayName} · {item.model}
                  {item.ready ? "" : "（不可用）"}
                </option>
              ))}
            </select>
          </label>
          <CustomModelPanel models={models} selected={selectedModelOption} onAdded={onModelChange} />
          <p className="text-xs leading-relaxed text-q-text-muted">
            开启 AI 时，每次检查都会按当前时间范围重新分析所选帖子，不跳过、不沿用旧结果；关闭时仅同步来源内容，结论区展示最近一次成功分析并标注时间。
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
          {analysisInput.length === 0 && contextInput.length === 0 ? (
            <p className="text-xs leading-relaxed text-q-text-muted">
              当前时间窗内没有可分析的帖子。Tibo 近期没有新动态时，可切换「自定义」扩大时间范围后重试。
            </p>
          ) : (
            <div className="flex min-h-0 flex-col gap-3">
              {contextInput.length > 0 && (
                <PostGroupPreview title="事件上下文" subtitle="已关联当前事件的历史原帖，只作背景" posts={contextInput} />
              )}
              <PostGroupPreview title="本次新增" subtitle="按所选时间范围" posts={analysisInput} />
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
        {data?.aiAssessment.enabled && data?.aiAssessment.state === "pending" ? (
          <p className="text-xs text-q-warning">有新动态待分析，下次「立即检查」时更新。</p>
        ) : null}
        {analysis?.conclusion ? (
          <>
            <div className="flex flex-col gap-1.5">
              <p className="text-[11px] font-semibold tracking-widest text-q-primary">结论</p>
              <p className="text-[15px] font-medium leading-relaxed text-q-text-primary" data-selectable="true">
                {analysis.conclusion}
              </p>
            </div>
            {analysis.analysisBasis ? (
              <div className="flex flex-col gap-1.5">
                <p className="text-[11px] font-semibold tracking-widest text-q-primary">
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
            <div className="grid grid-cols-[repeat(auto-fit,minmax(min(100%,320px),1fr))] gap-x-8 gap-y-3">
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
              为「{selected.displayName}」添加平台支持的任意模型名：先验证连接（发送一次极小请求），通过后保存即可加入上方下拉。
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
