import { useState } from "react";
import { CheckCircle2, ChevronDown, CircleAlert, LoaderCircle } from "lucide-react";
import { formatTime } from "@/lib/format";
import { cn } from "@/lib/cn";
import type { PlatformSummaryViewModel } from "@/lib/types";

/**
 * 最近刷新记录（Apple Glass V6）：直接展示后端 RefreshRun ViewModel。
 * 多账号平台在记录行展示账号名；旧记录 accountName 为空时降级为仅来源名。
 * - variant="panel"（宽屏右栏）：sticky 面板，列表超高在面板内部滚动；
 * - variant="inline"（中/紧凑单栏）：折叠卡，默认仅标题 + 最近一条 + 计数，
 *   展开后内滚展示全部（后端本就只返回最近 8 条）；收起/展开不清数据、不刷新。
 */
export function RefreshHistory({
  platform,
  variant = "panel",
}: {
  platform: PlatformSummaryViewModel;
  variant?: "panel" | "inline";
}) {
  const entries = platform.refreshHistory ?? [];
  const multiAccount = (platform.accounts?.length ?? 0) > 1;
  const [expanded, setExpanded] = useState(false);
  const latest = entries[0] ?? null;

  const list = (
    <ul
      className={cn(
        "flex flex-col gap-2.5",
        variant === "panel" && "min-h-0 flex-1 overflow-y-auto pr-0.5",
        variant === "inline" && !expanded && "hidden",
      )}
    >
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
          <div className="flex min-w-0 flex-1 flex-col gap-0.5">
            <div className="flex items-baseline justify-between gap-2">
              <p className="min-w-0 truncate text-[13px] font-medium text-q-text-primary">
                {entry.sourceName}
                {multiAccount && entry.accountName ? (
                  <span className="ml-1.5 rounded-q-pill bg-q-neutral-soft px-1.5 py-0.5 text-[10px] font-medium text-q-neutral">
                    {entry.accountName}
                  </span>
                ) : null}
              </p>
              <span className="shrink-0 text-[11px] tabular-nums text-q-text-muted">
                {entry.finishedAt ? formatTime(entry.finishedAt) : "进行中"}
              </span>
            </div>
            <p
              className={`text-[11px] leading-4 ${
                entry.status === "success"
                  ? "text-q-text-muted"
                  : entry.status === "partial"
                    ? "text-q-warning"
                    : entry.status === "running"
                      ? "text-q-primary"
                      : "text-q-danger"
              }`}
            >
              {entry.status === "success"
                ? "刷新成功"
                : entry.status === "partial"
                  ? "部分完成"
                  : entry.status === "running"
                    ? "刷新中"
                    : (entry.errorMessage ?? "刷新失败")}
            </p>
          </div>
        </li>
      ))}
    </ul>
  );

  if (variant === "panel") {
    return (
      <aside className="glass-panel sticky top-0 flex max-h-[min(100%,520px)] flex-col gap-3 self-start p-4">
        <p className="shrink-0 text-[13px] font-medium text-q-text-secondary">
          最近刷新记录
          {entries.length > 0 && (
            <span className="ml-1.5 text-[11px] text-q-text-muted">{entries.length} 条</span>
          )}
        </p>
        {list}
      </aside>
    );
  }

  return (
    <div className="glass-panel flex flex-col gap-2.5 p-4">
      <button
        type="button"
        aria-expanded={expanded}
        onClick={() => setExpanded((value) => !value)}
        className="flex cursor-pointer items-center gap-2 text-left"
      >
        <span className="text-[13px] font-medium text-q-text-secondary">最近刷新记录</span>
        {entries.length > 0 && (
          <span className="rounded-q-pill bg-q-neutral-soft px-1.5 py-0.5 text-[10px] tabular-nums text-q-neutral">
            {entries.length} 条
          </span>
        )}
        {!expanded && latest && (
          <span className="min-w-0 flex-1 truncate text-[11px] text-q-text-muted">
            {latest.sourceName}
            {multiAccount && latest.accountName ? ` · ${latest.accountName}` : ""} ·{" "}
            {latest.status === "success"
              ? "刷新成功"
              : latest.status === "partial"
                ? "部分完成"
                : latest.status === "running"
                  ? "刷新中"
                  : (latest.errorMessage ?? "刷新失败")}
          </span>
        )}
        <ChevronDown
          size={15}
          aria-hidden
          className={cn("ml-auto shrink-0 text-q-text-muted transition-transform duration-150", expanded && "rotate-180")}
        />
      </button>
      <div
        className={cn(
          variant === "inline" && expanded && "max-h-[320px] overflow-y-auto pr-0.5",
        )}
      >
        {list}
      </div>
    </div>
  );
}
