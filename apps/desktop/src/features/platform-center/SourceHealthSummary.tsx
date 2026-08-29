import { CheckCircle2, CircleAlert } from "lucide-react";
import { StatusBadge } from "@/components/ui/StatusBadge";
import { formatTime } from "@/lib/format";
import { SOURCE_STATE_META, type PlatformSummaryViewModel } from "@/lib/types";

/**
 * 来源健康摘要（Apple Glass V6）：逐行展示每个 Source 的运行状态、徽章与最近成功时间。
 * 单个 Source 失败不影响其他 Source 的展示（部分可用语义）。
 */
export function SourceHealthSummary({ platform }: { platform: PlatformSummaryViewModel }) {
  return (
    <div className="glass-panel flex flex-col gap-2.5 px-4 py-3.5">
      <p className="text-[13px] font-medium text-q-text-secondary">来源状态</p>
      <div className="flex flex-col gap-2">
        {platform.sources.map((source) => {
          const meta = SOURCE_STATE_META[source.state];
          const unconfigured = source.state === "auth_required" && !source.credentialConfigured;
          const failed = source.state === "error" || (source.state === "auth_required" && source.credentialConfigured);
          return (
            <div key={source.sourceId} className="flex min-w-0 flex-wrap items-center gap-x-2.5 gap-y-1">
              {failed ? (
                <CircleAlert size={15} className="shrink-0 text-q-danger" aria-hidden />
              ) : unconfigured ? (
                <CircleAlert size={15} className="shrink-0 text-q-neutral" aria-hidden />
              ) : (
                <CheckCircle2 size={15} className="shrink-0 text-q-success" aria-hidden />
              )}
              <span className="text-[13px] font-medium text-q-text-primary">{source.displayName}</span>
              <StatusBadge tone={meta.tone}>{meta.label}</StatusBadge>
              {source.state === "error" && source.errorMessage && (
                <span className="min-w-0 flex-1 truncate text-xs text-q-danger" title={source.errorMessage}>
                  {source.errorMessage}
                </span>
              )}
              {source.state === "ready" && source.lastSuccessAt && (
                <span className="ml-auto shrink-0 text-[11px] text-q-text-muted">
                  {formatTime(source.lastSuccessAt)}
                </span>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
