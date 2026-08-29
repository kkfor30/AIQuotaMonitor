/**
 * 悬浮详情内的 GPT 重置雷达二级页（设计稿 13）。
 * 在同一悬浮详情窗口内切换，复用窄版面板宽度；雷达结论始终标记为推测。
 */
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { ArrowLeft, ExternalLink, Languages } from "lucide-react";
import { ipcErrorMessage, openExternalUrl, translateRadarPost, type RadarPost, type RadarSnapshot } from "@/lib/ipc";
import { RADAR_SNAPSHOT_QUERY_KEY } from "@/lib/query-client";
import { formatHoverbarClock, radarConfidenceLabel } from "./hoverbar-state";

const POST_BADGE_LABEL: Record<string, string> = {
  RESET: "重置相关",
  BANKED: "已落地",
  LIMITS: "限制",
  VERIFYING: "重置相关",
  NOTE: "动态",
  无重置信号: "无重置信号",
  间接相关: "间接相关",
  重置相关: "重置相关",
};

export function HoverbarRadarDetail({
  radar,
  onBack,
}: {
  radar: RadarSnapshot | undefined;
  onBack: () => void;
}) {
  const queryClient = useQueryClient();
  const translate = useMutation({
    mutationFn: (postId: string) => translateRadarPost(postId),
    onSuccess: (snapshot) => queryClient.setQueryData(RADAR_SNAPSHOT_QUERY_KEY, snapshot),
  });

  const analysis = radar?.analysis;
  const latest = radar?.latest;
  const posts = (radar?.posts ?? []).slice(0, 3);

  return (
    <div className="hb-radar-page">
      <div className="hb-radar-top">
        <button type="button" className="hb-radar-back" onClick={onBack}>
          <ArrowLeft size={14} aria-hidden />
          返回额度
        </button>
        <b className="hb-radar-title">GPT 重置雷达</b>
        <span className="hb-radar-pill">仅为推测</span>
      </div>

      <section className="hb-radar-card">
        <div className="hb-radar-kv">
          <span>来源状态</span>
          <b data-status={radar?.sourceStatus ?? "missing"}>{sourceStatusLabel(radar?.sourceStatus)}</b>
        </div>
        <div className="hb-radar-kv">
          <span>最近检查</span>
          <b>{radar?.lastSyncedAt ? formatHoverbarClock(radar.lastSyncedAt) : "尚未同步"}</b>
        </div>
      </section>

      <section className="hb-radar-card">
        {analysis?.conclusion ? (
          <>
            <h3 className="hb-radar-card-title">AI 辅助结论</h3>
            <p className="hb-radar-text">{analysis.conclusion}</p>
            <p className="hb-radar-meta">
              把握度 {radarConfidenceLabel(analysis.confidence ?? "")} · 模型 {analysis.model ?? "—"}
            </p>
            <p className="hb-radar-meta">
              支持 {analysis.support.length} · 反向 {analysis.against.length} · 不确定 {analysis.uncertainty.length}
            </p>
          </>
        ) : (
          <>
            <h3 className="hb-radar-card-title">来源摘要</h3>
            {radar?.notice ? (
              <>
                <p className="hb-radar-text">{radar.notice.headline}</p>
                {radar.notice.lead ? <p className="hb-radar-meta">{radar.notice.lead}</p> : null}
                <p className="hb-radar-meta">CodexRadar 公告 · 未运行 AI 辅助分析</p>
              </>
            ) : latest ? (
              <>
                <p className="hb-radar-text">{latest.summary ?? latest.translatedText ?? latest.text}</p>
                <p className="hb-radar-meta">{formatHoverbarClock(latest.postedAt)} · 未运行 AI 辅助分析</p>
              </>
            ) : (
              <p className="hb-radar-meta">尚未同步，请在主窗口运行检查。</p>
            )}
          </>
        )}
      </section>

      <section className="hb-radar-card">
        <h3 className="hb-radar-card-title">最近 Tibo 动态</h3>
        {posts.length === 0 ? (
          <p className="hb-radar-meta">暂无动态。</p>
        ) : (
          posts.map((post) => (
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

      <p className="hb-radar-footnote">仅为推测，不代表官方结论；重置时间以官方实际执行为准。</p>
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
  return (
    <article className="hb-radar-post">
      <div className="hb-radar-post-head">
        <span className="hb-radar-post-badge">{POST_BADGE_LABEL[post.badge] ?? post.badge}</span>
        <span className="hb-radar-post-time">{formatHoverbarClock(post.postedAt)}</span>
      </div>
      <p className="hb-radar-post-text">{post.summary ?? post.translatedText ?? post.text}</p>
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
          <button type="button" className="hb-radar-post-button" onClick={() => void openExternalUrl(post.url)}>
            <ExternalLink size={12} aria-hidden />
            查看原文
          </button>
        ) : null}
      </div>
    </article>
  );
}

function sourceStatusLabel(status: string | undefined): string {
  if (status === "fresh") return "正常";
  if (status === "stale") return "缓存可能过期";
  return "未同步";
}
