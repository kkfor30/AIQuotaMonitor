import { useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Card } from "@/components/ui/Card";
import { Button } from "@/components/ui/Button";
import { cn } from "@/lib/cn";
import { fetchRadarSnapshot, ipcErrorMessage, openExternalUrl, runRadarCheck } from "@/lib/ipc";
import type { RadarPost } from "@/lib/ipc";

const RADAR_QUERY_KEY = ["radar-snapshot"] as const;

export function GptRadarPage() {
  const queryClient = useQueryClient();
  const [tab, setTab] = useState<RadarTabId>("signal");
  const [filter, setFilter] = useState<"all" | "signal" | "limits">("all");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [rangeKey, setRangeKey] = useState("3d");
  const [analyze, setAnalyze] = useState(false);
  const [sourceId, setSourceId] = useState<string>("");

  const { data, isLoading } = useQuery({
    queryKey: RADAR_QUERY_KEY,
    queryFn: fetchRadarSnapshot,
  });

  const checkMutation = useMutation({
    mutationFn: () =>
      runRadarCheck({
        analyze,
        rangeKey,
        sourceId: sourceId || data?.models.find((item) => item.ready)?.sourceId || null,
        model: data?.models.find((item) => item.sourceId === sourceId)?.model ?? null,
      }),
    onSuccess: (snapshot) => {
      queryClient.setQueryData(RADAR_QUERY_KEY, snapshot);
    },
  });

  const posts = data?.posts ?? [];
  const visible = posts.filter((post) => filter === "all" || post.filter === filter);
  const selected = visible.find((post) => post.id === selectedId) ?? visible[0] ?? null;
  const models = data?.models ?? [];

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-6">
      <header className="glass-panel flex flex-wrap items-start justify-between gap-3 px-5 py-4">
        <div>
          <div className="flex items-center gap-3">
            <h1 className="text-lg font-semibold tracking-tight text-q-text-primary">GPT 重置雷达</h1>
            <span className="rounded-q-pill bg-q-neutral-soft px-2.5 py-1 text-xs text-q-neutral">
              仅为推测，不代表官方结论
            </span>
          </div>
          <p className="mt-1 max-w-2xl text-[13px] leading-relaxed text-q-text-secondary">
            手动同步 Codex Radar 公开 feed。英文为 Tibo 原话转述；直接访问 X 留待后续。
          </p>
        </div>
        <Button onClick={() => checkMutation.mutate()} disabled={checkMutation.isPending}>
          {checkMutation.isPending ? "检查中…" : "立即检查"}
        </Button>
      </header>
      {checkMutation.error && (
        <p className="rounded-q-control border border-q-danger/25 bg-q-danger-soft px-3 py-2 text-xs text-q-danger">
          {ipcErrorMessage(checkMutation.error, "检查失败")}
        </p>
      )}

      <div role="tablist" className="inline-flex w-fit items-center gap-1 rounded-q-control border border-q-border bg-q-surface-muted p-1">
        {RADAR_TABS.map((item) => (
          <button
            key={item.id}
            type="button"
            role="tab"
            aria-selected={tab === item.id}
            onClick={() => setTab(item.id)}
            data-active={tab === item.id}
            className={cn(
              "cursor-pointer rounded-[7px] px-4 py-1.5 text-[13px] font-medium text-q-text-secondary hover:text-q-text-primary",
              "data-[active=true]:bg-q-surface-solid data-[active=true]:text-q-primary data-[active=true]:shadow-q-sm",
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

function SignalSummaryView({ data }: { data: Awaited<ReturnType<typeof fetchRadarSnapshot>> | undefined }) {
  const latest = data?.latest;
  const latestCheck = data?.checks[0];
  return (
    <div className="flex flex-col gap-4">
      <div className="grid grid-cols-1 gap-4 xl:grid-cols-3">
        <Card className="flex flex-col gap-2">
          <h2 className="text-sm font-semibold text-q-text-primary">Codex Radar 来源</h2>
          <p className="text-[13px] text-q-text-secondary">状态：{sourceLabel(data?.sourceStatus)}</p>
          <p className="text-xs text-q-text-muted">
            {data?.lastSyncedAt ? `上次同步 ${formatTime(data.lastSyncedAt)}` : "尚未同步，请点立即检查"}
          </p>
        </Card>
        <Card className="flex flex-col gap-2">
          <h2 className="text-sm font-semibold text-q-text-primary">最新动态</h2>
          {latest ? (
            <>
              <p className="text-xs font-medium text-q-primary">{latest.badge}</p>
              <p className="text-[13px] leading-relaxed text-q-text-primary">{latest.text}</p>
              <p className="text-xs text-q-text-muted">{formatTime(latest.postedAt)}</p>
            </>
          ) : (
            <p className="text-xs text-q-text-muted">暂无动态</p>
          )}
        </Card>
        <Card className="flex flex-col gap-2">
          <h2 className="text-sm font-semibold text-q-text-primary">AI 辅助结论</h2>
          {data?.analysis?.conclusion ? (
            <>
              <p className="text-[13px] text-q-text-primary">{data.analysis.conclusion}</p>
              <p className="text-xs text-q-text-muted">把握度 {data.analysis.confidence ?? "—"}</p>
            </>
          ) : (
            <p className="text-xs text-q-text-muted">未分析。可在 AI 辅助分析 Tab 开启后随立即检查运行。</p>
          )}
        </Card>
      </div>
      <Card>
        <h2 className="text-sm font-semibold text-q-text-primary">检查流程</h2>
        <p className="mt-2 text-xs text-q-text-secondary">
          同步 {latestCheck?.syncStatus ?? "—"} · 解析 {latestCheck?.parseStatus ?? "—"} · 分析 {latestCheck?.analyzeStatus ?? "—"}
        </p>
        {latestCheck?.errorMessage && <p className="mt-2 text-xs text-q-danger">{latestCheck.errorMessage}</p>}
      </Card>
      <Card>
        <h2 className="text-sm font-semibold text-q-text-primary">检查历史</h2>
        <div className="mt-3 space-y-2">
          {(data?.checks ?? []).length === 0 && <p className="text-xs text-q-text-muted">还没有检查记录</p>}
          {(data?.checks ?? []).map((check) => (
            <p key={check.id} className="text-xs text-q-text-secondary">
              {formatTime(check.startedAt)} · {check.status} · {check.postCount} 条
              {check.errorMessage ? ` · ${check.errorMessage}` : ""}
            </p>
          ))}
        </div>
      </Card>
    </div>
  );
}

function TiboFeedView({
  posts,
  selected,
  filter,
  onFilter,
  onSelect,
}: {
  posts: RadarPost[];
  selected: RadarPost | null;
  filter: "all" | "signal" | "limits";
  onFilter: (value: "all" | "signal" | "limits") => void;
  onSelect: (id: string) => void;
}) {
  return (
    <div className="grid min-h-[360px] grid-cols-1 gap-4 xl:grid-cols-[minmax(0,1fr)_minmax(0,1.2fr)]">
      <Card className="flex min-h-0 flex-col gap-3">
        <div className="flex items-center justify-between gap-2">
          <h2 className="text-sm font-semibold text-q-text-primary">Tibo 动态</h2>
          <div className="flex gap-1">
            {[
              ["all", "全部"],
              ["signal", "信号"],
              ["limits", "限制"],
            ].map(([id, label]) => (
              <button
                key={id}
                type="button"
                onClick={() => onFilter(id as typeof filter)}
                className={cn(
                  "rounded-q-pill px-2.5 py-1 text-xs",
                  filter === id ? "bg-q-primary text-white" : "bg-q-neutral-soft text-q-text-secondary",
                )}
              >
                {label}
              </button>
            ))}
          </div>
        </div>
        <div className="min-h-0 flex-1 space-y-2 overflow-y-auto">
          {posts.length === 0 && <p className="text-xs text-q-text-muted">点「立即检查」同步 Codex Radar。</p>}
          {posts.map((post) => (
            <button
              key={post.id}
              type="button"
              onClick={() => onSelect(post.id)}
              className={cn(
                "w-full rounded-q-control border px-3 py-2.5 text-left",
                selected?.id === post.id ? "border-q-border-selected bg-q-primary-softer" : "border-q-border hover:bg-q-surface-hover",
              )}
            >
              <div className="flex items-center justify-between gap-2">
                <span className="text-[11px] font-bold text-q-primary">{post.badge}</span>
                <span className="text-[11px] text-q-text-muted">{formatTime(post.postedAt)}</span>
              </div>
              <p className="mt-1 line-clamp-3 text-[13px] leading-relaxed text-q-text-primary">{post.text}</p>
              <p className="mt-1 text-[11px] text-q-text-muted">
                {post.replies} 回复 · {post.reposts} 转发 · {post.likes} 喜欢
              </p>
            </button>
          ))}
        </div>
      </Card>
      <Card className="flex flex-col gap-3">
        <h2 className="text-sm font-semibold text-q-text-primary">动态详情</h2>
        {selected ? (
          <>
            <p className="text-xs font-medium text-q-primary">{selected.badge}</p>
            <p className="whitespace-pre-wrap text-[13px] leading-relaxed text-q-text-primary">{selected.text}</p>
            <p className="text-xs text-q-text-muted">{formatTime(selected.postedAt)}</p>
            <div className="mt-auto flex gap-2">
              {selected.url && (
                <Button variant="secondary" size="sm" onClick={() => void openExternalUrl(selected.url)}>
                  打开 X 原帖
                </Button>
              )}
              <Button variant="ghost" size="sm" onClick={() => void openExternalUrl("https://codex-reset.com/tibo")}>
                Codex Radar 来源
              </Button>
            </div>
          </>
        ) : (
          <p className="text-xs text-q-text-muted">从左侧选择一条动态</p>
        )}
      </Card>
    </div>
  );
}

function AiAnalysisView({
  data,
  analyze,
  rangeKey,
  sourceId,
  models,
  onAnalyzeChange,
  onRangeChange,
  onSourceChange,
}: {
  data: Awaited<ReturnType<typeof fetchRadarSnapshot>> | undefined;
  analyze: boolean;
  rangeKey: string;
  sourceId: string;
  models: { sourceId: string; displayName: string; model: string; ready: boolean }[];
  onAnalyzeChange: (value: boolean) => void;
  onRangeChange: (value: string) => void;
  onSourceChange: (value: string) => void;
}) {
  const analysis = data?.analysis;
  const cut = data?.cut;
  const analysisInput = useMemo(() => {
    const start = rangeStartMs(rangeKey);
    return (data?.posts ?? []).filter(
      (post) => post.postedAt >= start && (!cut || post.postedAt > cut.postedAt),
    );
  }, [cut, data?.posts, rangeKey]);
  return (
    <div className="flex flex-col gap-4">
      <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
        <Card className="flex flex-col gap-3">
          <h2 className="text-sm font-semibold text-q-text-primary">分析配置</h2>
          <label className="flex items-center gap-2 text-sm text-q-text-secondary">
            <input type="checkbox" checked={analyze} onChange={(event) => onAnalyzeChange(event.target.checked)} />
            立即检查时同时运行 AI 分析
          </label>
          <label className="text-sm text-q-text-secondary">
            时间范围
            <select
              value={rangeKey}
              onChange={(event) => onRangeChange(event.target.value)}
              className="mt-1 h-10 w-full rounded-q-control border border-q-border bg-q-surface px-3"
            >
              <option value="today">当天</option>
              <option value="3d">过去 3 天</option>
              <option value="7d">过去 7 天</option>
            </select>
          </label>
          <label className="text-sm text-q-text-secondary">
            分析模型
            <select
              value={sourceId}
              onChange={(event) => onSourceChange(event.target.value)}
              className="mt-1 h-10 w-full rounded-q-control border border-q-border bg-q-surface px-3"
            >
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
            默认只分析「上次已落地重置」之后的帖，避免把旧重置当成新信号。
            {cut ? ` 当前切点：${formatTime(cut.postedAt)}` : " 还没有切点。"}
          </p>
        </Card>
        <Card className="flex flex-col gap-2">
          <h2 className="text-sm font-semibold text-q-text-primary">本次输入</h2>
          <p className="text-xs leading-relaxed text-q-text-muted">
            只发送英文原文、时间和原帖链接。不发送翻译、上游标签、账号或凭据。
            上次已落地重置之后的帖才会进入分析。
          </p>
          {analysisInput.length === 0 ? (
            <p className="text-xs text-q-text-muted">当前时间窗内没有可分析的新帖。</p>
          ) : (
            <div className="max-h-48 space-y-2 overflow-y-auto">
              <p className="text-xs text-q-text-secondary">将发送 {analysisInput.length} 条</p>
              {analysisInput.slice(0, 6).map((post) => (
                <p key={post.id} className="text-[12px] leading-relaxed text-q-text-primary">
                  {formatTime(post.postedAt)} · {post.text}
                </p>
              ))}
            </div>
          )}
        </Card>
      </div>
      <Card className="flex flex-col gap-3">
        <h2 className="text-sm font-semibold text-q-text-primary">辅助结论</h2>
        {analysis?.errorMessage && <p className="text-xs text-q-danger">{analysis.errorMessage}</p>}
        {analysis?.conclusion ? (
          <>
            <p className="text-[13px] leading-relaxed text-q-text-primary">{analysis.conclusion}</p>
            <p className="text-xs text-q-text-muted">把握度 {analysis.confidence} · {analysis.model}</p>
            <ListBlock title="引用" items={analysis.citations} />
            <ListBlock title="支持依据" items={analysis.support} />
            <ListBlock title="反向依据" items={analysis.against} />
            <ListBlock title="不确定性" items={analysis.uncertainty} />
          </>
        ) : (
          <p className="text-xs text-q-text-muted">尚未生成分析。勾选 AI 后点立即检查。</p>
        )}
      </Card>
    </div>
  );
}

function ListBlock({ title, items }: { title: string; items: string[] }) {
  if (items.length === 0) return null;
  return (
    <div>
      <p className="text-xs font-medium text-q-text-secondary">{title}</p>
      <ul className="mt-1 list-disc pl-4 text-xs leading-relaxed text-q-text-primary">
        {items.map((item) => (
          <li key={item}>{item}</li>
        ))}
      </ul>
    </div>
  );
}

function sourceLabel(status?: string) {
  if (status === "fresh") return "正常";
  if (status === "stale") return "缓存";
  return "未同步";
}

function formatTime(value: number) {
  return new Date(value).toLocaleString("zh-CN", { hour12: false });
}

function rangeStartMs(rangeKey: string) {
  if (rangeKey === "today") {
    const start = new Date();
    start.setHours(0, 0, 0, 0);
    return start.getTime();
  }
  const days = rangeKey === "3d" ? 3 : 7;
  return Date.now() - days * 24 * 60 * 60 * 1000;
}
