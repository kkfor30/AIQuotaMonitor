import { CheckCircle2, CircleAlert, LoaderCircle } from "lucide-react";
import { formatTime } from "@/lib/format";
import type { PlatformSummaryViewModel } from "@/lib/types";

/**
 * 刷新记录：直接展示后端 RefreshRun ViewModel。
 */
export function RefreshHistory({ platform }: { platform: PlatformSummaryViewModel }) {
  const entries = platform.refreshHistory ?? [];

  return (
    <div className="glass-panel flex flex-col gap-3 p-4">
      <p className="text-[13px] font-medium text-q-text-secondary">最近刷新记录</p>
      <ul className="flex flex-col gap-2.5">
        {entries.length === 0 ? (
          <li className="text-xs text-q-text-muted">暂无真实刷新记录</li>
        ) : entries.map((entry) => (
          <li key={entry.id} className="flex items-start gap-2.5">
            {entry.status === "success" ? (
              <CheckCircle2 size={15} className="mt-0.5 shrink-0 text-q-success" aria-hidden />
            ) : entry.status === "running" ? (
              <LoaderCircle size={15} className="mt-0.5 shrink-0 animate-spin text-q-primary" aria-hidden />
            ) : (
              <CircleAlert
                size={15}
                className={`mt-0.5 shrink-0 ${entry.status === "partial" ? "text-q-warning" : "text-q-danger"}`}
                aria-hidden
              />
            )}
            <div className="min-w-0">
              <p className="text-[13px] text-q-text-primary">
                {entry.sourceName}
                <span className="ml-2 text-xs text-q-text-muted">{entry.finishedAt ? formatTime(entry.finishedAt) : "进行中"}</span>
              </p>
              <p className={`text-xs ${entry.status === "success" ? "text-q-text-muted" : entry.status === "partial" ? "text-q-warning" : entry.status === "running" ? "text-q-primary" : "text-q-danger"}`}>
                {entry.status === "success" ? "刷新成功" : entry.status === "partial" ? "部分完成" : entry.status === "running" ? "刷新中" : (entry.errorMessage ?? "刷新失败")}
              </p>
            </div>
          </li>
        ))}
      </ul>
    </div>
  );
}
