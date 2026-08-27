import { CheckCircle2, CircleAlert } from "lucide-react";
import { formatTime } from "@/lib/format";
import type { PlatformSummaryViewModel } from "@/lib/types";

/**
 * 刷新记录：阶段一由静态 Source 摘要推导（无真实 RefreshRun 表）。
 * 成功/失败同时使用图标 + 文字表达。
 */
export function RefreshHistory({ platform }: { platform: PlatformSummaryViewModel }) {
  const entries = platform.sources.map((source) => ({
    id: source.sourceId,
    name: source.displayName,
    ok: source.state === "ready",
    at: source.state === "ready" ? source.lastSuccessAt : source.lastValidatedAt,
    detail:
      source.state === "ready"
        ? "刷新成功"
        : (source.errorMessage ?? "刷新失败"),
  }));

  return (
    <div className="glass-panel flex flex-col gap-3 p-4">
      <p className="text-[13px] font-medium text-q-text-secondary">最近刷新记录</p>
      <ul className="flex flex-col gap-2.5">
        {entries.map((entry) => (
          <li key={entry.id} className="flex items-start gap-2.5">
            {entry.ok ? (
              <CheckCircle2 size={15} className="mt-0.5 shrink-0 text-q-success" aria-hidden />
            ) : (
              <CircleAlert size={15} className="mt-0.5 shrink-0 text-q-danger" aria-hidden />
            )}
            <div className="min-w-0">
              <p className="text-[13px] text-q-text-primary">
                {entry.name}
                <span className="ml-2 text-xs text-q-text-muted">{formatTime(entry.at)}</span>
              </p>
              <p className={`text-xs ${entry.ok ? "text-q-text-muted" : "text-q-danger"}`}>
                {entry.detail}
              </p>
            </div>
          </li>
        ))}
      </ul>
      <p className="mt-auto text-[11px] text-q-text-muted">
        阶段一为静态演示数据；刷新流水将在阶段二接入 SQLite 后记录。
      </p>
    </div>
  );
}
