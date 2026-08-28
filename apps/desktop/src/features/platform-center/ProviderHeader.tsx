import { ExternalLink, RefreshCw } from "lucide-react";
import { PlatformMark } from "./ProviderRail";
import { AggregateStatusBadge } from "@/components/ui/StatusBadge";
import { Button } from "@/components/ui/Button";
import type { PlatformSummaryViewModel } from "@/lib/types";

/**
 * 页面头部：当前平台、接入方式摘要、聚合状态与操作区。
 */
export function ProviderHeader({
  platform,
  refreshing,
  onRefresh,
}: {
  platform: PlatformSummaryViewModel;
  refreshing: boolean;
  onRefresh: () => void;
}) {
  return (
    <header className="flex items-start justify-between gap-4">
      <div className="flex items-center gap-4">
        <PlatformMark providerId={platform.providerId} size={44} />
        <div>
          <div className="flex items-center gap-3">
            <h1 className="text-xl font-semibold tracking-tight text-q-text-primary">
              {platform.displayName}
            </h1>
            <AggregateStatusBadge status={platform.aggregateStatus} />
          </div>
          <p className="mt-1 text-[13px] text-q-text-secondary">{platform.accessSummary}</p>
        </div>
      </div>

      <div className="flex items-center gap-2">
        <Button
          variant="secondary"
          size="sm"
          disabled={!platform.officialUrl}
          onClick={() => platform.officialUrl && window.open(platform.officialUrl, "_blank", "noopener,noreferrer")}
          title={platform.officialUrl ? "打开官方页面" : "暂未提供官方页面"}
        >
          <ExternalLink size={15} aria-hidden />
          官方页面
        </Button>
        <Button size="sm" onClick={onRefresh} disabled={refreshing}>
          <RefreshCw size={15} aria-hidden className={refreshing ? "animate-spin" : ""} />
          {refreshing ? "刷新中…" : "刷新平台"}
        </Button>
      </div>
    </header>
  );
}
