import { SourceCard } from "./SourceCard";
import { EmptyState } from "@/components/ui/EmptyState";
import type { PlatformSummaryViewModel } from "@/lib/types";

/**
 * 平台中心 / 接入与来源：
 * 一个「来源」对应一个独立获取能力；每个 Source 独立保存凭据状态与验证结果。
 * 编辑抽屉（覆盖式）与保存前验证在阶段二实现。
 * focusSourceId 用于从总览关注项定位并高亮某个 Source。
 */
export function SourcesView({
  platform,
  focusSourceId,
}: {
  platform: PlatformSummaryViewModel;
  focusSourceId?: string;
}) {
  if (platform.aggregateStatus === "setup_required") {
    return (
      <EmptyState
        title={`接入 ${platform.displayName}`}
        description="为该平台添加 API Key、网页会话或其他来源。每种来源独立验证与刷新，互不影响。"
      />
    );
  }

  return (
    <div className="min-h-0 flex-1 overflow-y-auto pr-1">
      <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
        {platform.sources.map((source) => (
          <SourceCard
            key={source.sourceId}
            source={source}
            focused={source.sourceId === focusSourceId}
          />
        ))}
      </div>
      <p className="px-1 py-3 text-xs text-q-text-muted">
        阶段一展示静态来源状态；来源编辑抽屉、保存前验证与凭据安全存储在阶段二实现。
      </p>
    </div>
  );
}
