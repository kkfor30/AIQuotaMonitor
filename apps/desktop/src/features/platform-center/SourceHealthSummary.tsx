import { CheckCircle2, CircleAlert } from "lucide-react";
import { StatusBadge } from "@/components/ui/StatusBadge";
import { formatTime } from "@/lib/format";
import { SOURCE_STATE_META, type PlatformSummaryViewModel } from "@/lib/types";

/**
 * 来源健康摘要：页面顶部横条，逐 Source 展示运行状态与错误。
 * 单个 Source 失败不影响其他 Source 的展示（部分可用语义）。
 */
export function SourceHealthSummary({ platform }: { platform: PlatformSummaryViewModel }) {
  return (
    <div className="glass-panel flex flex-wrap items-center gap-x-6 gap-y-2 px-4 py-3">
      <span className="text-[13px] font-medium text-q-text-secondary">来源状态</span>
      {platform.sources.map((source) => {
        const meta = SOURCE_STATE_META[source.state];
        const failed = source.state === "error" || source.state === "auth_required";
        return (
          <div key={source.sourceId} className="flex items-center gap-2">
            {failed ? (
              <CircleAlert size={15} className="text-q-danger" aria-hidden />
            ) : (
              <CheckCircle2 size={15} className="text-q-success" aria-hidden />
            )}
            <span className="text-[13px] text-q-text-primary">{source.displayName}</span>
            <StatusBadge tone={meta.tone}>{meta.label}</StatusBadge>
            {source.state === "error" && source.errorMessage && (
              <span className="text-xs text-q-danger" title={source.errorMessage}>
                {source.errorMessage}
              </span>
            )}
            {source.state === "ready" && source.lastSuccessAt && (
              <span className="text-xs text-q-text-muted">{formatTime(source.lastSuccessAt)}</span>
            )}
          </div>
        );
      })}
    </div>
  );
}
