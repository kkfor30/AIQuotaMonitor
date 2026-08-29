import { useEffect, useMemo, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Card } from "@/components/ui/Card";
import { Button } from "@/components/ui/Button";
import { cn } from "@/lib/cn";
import { fetchRadarSnapshot, ipcErrorMessage, openExternalUrl, runRadarCheck, saveRadarAnalysisPrefs } from "@/lib/ipc";
import { RADAR_SNAPSHOT_QUERY_KEY } from "@/lib/query-client";
import type { RadarPost } from "@/lib/ipc";

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
            手动同步 CodexRadar 公开首页的 Tibo 动态与中文翻译。我们自己的 AI 分析默认关闭，且只使用英文原文。
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

function SignalSummaryView({ data }: { data: Awaited<ReturnType<typeof fetchRadarSnapshot>> | undefined }) {
  const latest = data?.latest;
  const notice = data?.notice;
  const latestCheck = data?.checks[0];
  return (
    <div className="flex flex-col gap-4">
      <div className="grid grid-cols-1 gap-4 xl:grid-cols-3">
        <Card className="flex flex-col gap-2">
          <h2 className="text-sm font-semibold text-q-text-primary">CodexRadar 来源</h2>
          <p className="text-[13px] text-q-text-secondary">状态：{sourceLabel(data?.sourceStatus)}</p>
          <p className="text-xs text-q-text-muted">
            {data?.lastSyncedAt ? `上次同步 ${formatTime(data.lastSyncedAt)}` : "尚未同步，请点立即检查"}
          </p>
          <Button variant="ghost" size="sm" onClick={() => void openExternalUrl("https://codexradar.com/")}>
            打开 CodexRadar
          </Button>
        </Card>
        <Card className="flex flex-col gap-2">
          <h2 className="text-sm font-semibold text-q-text-primary">顶部公告</h2>
          {notice ? (
            <>
              <p className="text-[13px] font-medium leading-relaxed text-q-text-primary">{notice.headline}</p>
              {notice.lead ? <p className="text-xs text-q-text-secondary">{notice.lead}</p> : null}
              {notice.items[0] ? <p className="text-xs leading-relaxed text-q-text-muted">{notice.items[0]}</p> : null}
            </>
          ) : latest ? (
            <>
              <p className="text-xs font-medium text-q-primary">{latest.badge}</p>
              <p className="text-[13px] leading-relaxed text-q-text-primary">
                {latest.summary ?? latest.translatedText ?? latest.text}
              </p>
              <p className="text-xs text-q-text-muted">{formatTime(latest.postedAt)}</p>
            </>
          ) : (
            <p className="text-xs text-q-text-muted">暂无公告</p>
          )}
        </Card>
        <Card className="flex flex-col gap-2">
          <h2 className="text-sm font-semibold text-q-text-primary">AI 辅助结论</h2>
          {data?.analysis?.errorMessage ? (
            <p className="text-xs leading-relaxed text-q-danger">{data.analysis.errorMessage}</p>
          ) : null}
          {data?.analysis?.conclusion ? (
            <>
              <p className="text-[13px] text-q-text-primary">{data.analysis.conclusion}</p>
              <p className="text-xs text-q-text-muted">把握度 {data.analysis.confidence ?? "—"}</p>
            </>
          ) : (
            <p className="text-xs text-q-text-muted">
              {data?.analysis?.errorMessage
                ? "本次分析失败，可更换模型后重试。"
                : "未分析。可在 AI 辅助分析 Tab 开启后随立即检查运行。"}
            </p>
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
  filter: "all" | "related" | "none";
  onFilter: (value: "all" | "related" | "none") => void;
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
              ["related", "重置相关"],
              ["none", "无重置信号"],
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
          {posts.length === 0 && <p className="text-xs text-q-text-muted">点「立即检查」同步 CodexRadar。</p>}
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
              <p className="mt-1 line-clamp-3 text-[13px] leading-relaxed text-q-text-primary">
                {post.summary ?? post.translatedText ?? post.text}
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
            <p className="text-xs text-q-text-muted">{formatTime(selected.postedAt)}</p>
            <div>
              <p className="text-xs font-medium text-q-text-secondary">中文翻译</p>
              <p className="mt-1 whitespace-pre-wrap text-[13px] leading-relaxed text-q-text-primary">
                {selected.translatedText ?? selected.summary ?? "尚无中文翻译"}
              </p>
            </div>
            <details>
              <summary className="cursor-pointer text-xs text-q-text-secondary">英文原文</summary>
              <p className="mt-1 whitespace-pre-wrap text-[13px] leading-relaxed text-q-text-muted">{selected.text}</p>
            </details>
            {selected.analysis ? (
              <div className="rounded-q-control border border-q-border bg-q-surface-muted/70 px-3 py-2">
                <p className="text-xs font-medium text-q-text-secondary">CodexRadar 解读 · 仅为上游参考</p>
                <p className="mt-1 text-[13px] leading-relaxed text-q-text-primary">{selected.analysis}</p>
              </div>
            ) : null}
            <div className="mt-auto flex gap-2">
              {selected.url && (
                <Button variant="secondary" size="sm" onClick={() => void openExternalUrl(selected.url)}>
                  打开 X 原帖
                </Button>
              )}
              <Button variant="ghost" size="sm" onClick={() => void openExternalUrl("https://codexradar.com/")}>
                CodexRadar 来源
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
    const start = rangeStartMs(rangeKey);
    return (data?.posts ?? []).filter((post) => post.postedAt >= start);
  }, [data?.posts, rangeKey]);
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
            每次检查都会按当前时间范围重新分析所选帖子，不跳过、不沿用旧结果。
          </p>
        </Card>
        <Card className="flex flex-col gap-2">
          <h2 className="text-sm font-semibold text-q-text-primary">本次输入</h2>
          <p className="text-xs leading-relaxed text-q-text-muted">
            只发送英文原文、时间和原帖链接。不发送 CodexRadar 的中文翻译、信号标签或模型语境解读。
          </p>
          {analysisInput.length === 0 ? (
            <p className="text-xs text-q-text-muted">当前时间窗内没有可分析的帖子。</p>
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
      <Card className="flex flex-col gap-2">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <h2 className="text-sm font-semibold text-q-text-primary">语义提示</h2>
          <button
            type="button"
            className="text-xs font-medium text-q-primary hover:underline"
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
          className="min-h-[132px] w-full resize-y rounded-q-control border border-q-border bg-q-surface px-3 py-2 text-[13px] leading-relaxed text-q-text-primary"
        />
        <p className="text-[11px] text-q-text-muted">{userPrompt.length}/4000</p>
      </Card>
      <Card className="flex flex-col gap-3">
        <h2 className="text-sm font-semibold text-q-text-primary">辅助结论</h2>
        {analyzeError ? <p className="text-xs leading-relaxed text-q-danger">{analyzeError}</p> : null}
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
          <p className="text-xs text-q-text-muted">
            {analyzeError ? "分析未完成，请更换模型或稍后重试。" : "尚未生成分析。勾选 AI 后点立即检查。"}
          </p>
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
