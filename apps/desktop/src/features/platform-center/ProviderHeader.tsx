import { ExternalLink, RefreshCw, Trash2 } from "lucide-react";
import { PlatformMark } from "./ProviderRail";
import { AggregateStatusBadge } from "@/components/ui/StatusBadge";
import { Button } from "@/components/ui/Button";
import { ipcErrorMessage, openExternalUrl } from "@/lib/ipc";
import type { PlatformSummaryViewModel } from "@/lib/types";

/**
 * 页面头部（Apple Glass V6）：当前平台、接入方式摘要、聚合状态与操作区。
 */
export function ProviderHeader({
  platform,
  refreshing,
  onRefresh,
  onRemove,
}: {
  platform: PlatformSummaryViewModel;
  refreshing: boolean;
  onRefresh: () => void;
  onRemove: () => void;
}) {
  return (
    <header className="flex flex-wrap items-start justify-between gap-x-4 gap-y-3 px-1">
      <div className="flex min-w-0 items-center gap-4">
        <PlatformMark providerId={platform.providerId} size={56} />
        <div className="min-w-0">
          <div className="flex items-center gap-3">
            <h1 className="min-w-0 truncate text-[24px] font-bold tracking-tight text-q-text-primary">
              {platform.displayName}
            </h1>
            <AggregateStatusBadge status={platform.aggregateStatus} />
          </div>
          <p className="mt-1 truncate text-[13px] text-q-text-secondary">{platform.accessSummary}</p>
        </div>
      </div>

      <div className="flex shrink-0 flex-wrap items-center justify-end gap-2">
        <Button
          variant="secondary"
          size="sm"
          disabled={!platform.officialUrl}
          onClick={() => {
            if (!platform.officialUrl) return;
            void openExternalUrl(platform.officialUrl).catch((cause) => {
              console.error(ipcErrorMessage(cause, "无法打开官方页面。"));
            });
          }}
          title={platform.officialUrl ? "打开官方页面" : "暂未提供官方页面"}
        >
          <ExternalLink size={15} aria-hidden />
          官方页面
        </Button>
        <Button variant="ghost" size="sm" onClick={onRemove} title="移除该平台">
          <Trash2 size={15} aria-hidden />
          移除平台
        </Button>
        <Button size="sm" onClick={onRefresh} disabled={refreshing}>
          <RefreshCw size={15} aria-hidden className={refreshing ? "animate-spin" : ""} />
          {refreshing ? "刷新中…" : "刷新平台"}
        </Button>
      </div>
    </header>
  );
}
